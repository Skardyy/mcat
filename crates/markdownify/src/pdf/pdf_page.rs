use std::{collections::BTreeMap, error::Error, mem::take, str::from_utf8};

use lopdf::{Dictionary, Document, Encoding, Object, ObjectId};

use crate::pdf::pdf_element::is_fake_line;

use super::{
    pdf_element::{PdfLine, PdfText, PdfUnit},
    pdf_state::PdfState,
};

pub struct PdfPage<'a> {
    pub stream: Vec<u8>,
    document: &'a Document,
    fonts: BTreeMap<Vec<u8>, &'a Dictionary>,
    encodings: BTreeMap<Vec<u8>, Encoding<'a>>,
    resource: Option<&'a Dictionary>,
    current_font_alias: Vec<u8>,
    current_font_name: Option<String>,
    current_font_size: Option<f32>,
    state: PdfState,
    state_stack: Vec<PdfState>,
    pub child_fonts: Option<BTreeMap<Vec<u8>, &'a Dictionary>>,
    pub child_encodings: Option<BTreeMap<Vec<u8>, Encoding<'a>>>,
}

impl<'a> PdfPage<'a> {
    pub fn from_object_id(doc: &'_ Document, id: ObjectId) -> Result<PdfPage<'_>, Box<dyn Error>> {
        let stream = doc.get_page_content(id)?;
        let fonts = doc.get_page_fonts(id)?;
        let encodings: BTreeMap<Vec<u8>, Encoding> = fonts
            .clone()
            .into_iter()
            .filter_map(|(name, font)| match font.get_font_encoding(&doc) {
                Ok(it) => Some((name, it)),
                Err(_) => None,
            })
            .collect();

        let resource = doc.get_page_resources(id).ok().and_then(|r| {
            let res = match r.0 {
                Some(v) => Some(v),
                None => r.1.iter().find_map(|obj_id| {
                    let obj = doc.get_object(*obj_id).ok()?;
                    obj.as_dict().ok()
                }),
            };
            res
        });

        Ok(PdfPage {
            stream,
            fonts,
            resource,
            document: doc,
            encodings,
            current_font_alias: Vec::new(),
            state: PdfState::new(),
            state_stack: Vec::new(),
            current_font_name: None,
            current_font_size: None,
            child_fonts: None,
            child_encodings: None,
        })
    }

    pub fn handle_stream(&mut self, stream: Vec<u8>) -> Result<Vec<PdfUnit>, Box<dyn Error>> {
        let mut elements: Vec<PdfUnit> = Vec::new();
        let mut current_element = PdfText::default();
        let content = lopdf::content::Content::decode(&stream)?;
        let mut pending_lines = Vec::new();

        let _: Vec<Result<(), Box<dyn Error>>> = content
            .operations
            .iter()
            .map(|op| -> Result<(), Box<dyn Error>> {
                match op.operator.as_ref() {
                    "Tj" | "TJ" | "'" | "\"" => {
                        // ' is like TJ just with T* before it
                        // " is like as ' just with aw and ac as the first 2 operands
                        let r: &str = op.operator.as_ref();
                        if r == "'" || r == "\"" {
                            self.state.t_star();
                        }
                        // currently ignores spacing in between chars. may include joined words
                        // because of weird pdfs design.
                        let text = self.extract_text_from_objs(&op.operands);
                        let (x, y) = self.state.current_position();
                        current_element.text = text;
                        current_element.x = x;
                        current_element.y = y;
                        current_element.italic = self.state.tm.c != 0.0;
                        current_element.font_name = self.current_font_name.clone();
                        current_element.font_size =
                            self.current_font_size.map(|s| s * self.state.tm.a);
                        elements.push(PdfUnit::Text(take(&mut current_element)));
                        Ok(())
                    }
                    "Do" => {
                        // image or form.
                        let obj = op
                            .operands
                            .get(0)
                            .ok_or("failed to query xobject from 'Do' operator")?;

                        let obj_name = obj.as_name()?;
                        if from_utf8(obj_name).unwrap_or_default().contains("Im") {
                            return Ok(());
                        }

                        let resource = self.resource.ok_or("failed to query resource from pdf")?;
                        let dict = resource.get(b"XObject")?.as_dict()?;
                        let id = dict.get(obj_name)?.as_reference()?;
                        let stream = self.document.get_object(id)?.as_stream()?;
                        let raw = stream.decompressed_content()?;

                        let mut child_fonts: BTreeMap<Vec<u8>, &Dictionary> = BTreeMap::new();
                        if let Ok(dict) = stream.dict.get(b"Resources").and_then(Object::as_dict) {
                            if let Ok(fonts) = dict.get(b"Font").and_then(Object::as_dict) {
                                for (k, v) in fonts.iter() {
                                    if let Some(font_dict) = v
                                        .as_reference()
                                        .ok()
                                        .and_then(|id| self.document.get_object(id).ok())
                                        .and_then(|obj| obj.as_dict().ok())
                                    {
                                        child_fonts.insert(k.clone(), font_dict);
                                    }
                                }
                            }
                        }
                        let child_encodings: BTreeMap<Vec<u8>, Encoding> = child_fonts
                            .iter()
                            .filter_map(|(name, font)| {
                                match font.get_font_encoding(&self.document) {
                                    Ok(enc) => Some((name.clone(), enc)),
                                    Err(_) => None,
                                }
                            })
                            .collect();
                        self.child_fonts = Some(child_fonts);
                        self.child_encodings = Some(child_encodings);

                        let units = self.handle_stream(raw)?;
                        elements.extend(units);
                        Ok(())
                    }
                    "BT" => {
                        // begin text
                        self.state.bt();
                        Ok(())
                    }
                    "q" => {
                        // push state to the stack
                        self.state_stack.push(self.state.clone());
                        Ok(())
                    }
                    "Q" => {
                        // pop state from the stack
                        self.state = self.state_stack.pop().unwrap_or_default();
                        Ok(())
                    }
                    "ET" => {
                        // end text
                        self.state.et();
                        Ok(())
                    }
                    "Tf" => {
                        // font info
                        let items = op.operands.get(..2).ok_or("failed to query font info")?;
                        let font_alias = items[0].as_name()?;
                        self.current_font_alias = font_alias.to_owned();

                        // styles realted to fonts~
                        self.current_font_size = items[1].as_float().ok();
                        let font_info = self
                            .fonts
                            .get(font_alias)
                            .ok_or("failed to get fonts for page")?;
                        let font_name = font_info.get(b"BaseFont")?;
                        self.current_font_name =
                            String::from_utf8(font_name.as_name()?.to_vec()).ok();
                        Ok(())
                    }
                    "cm" => {
                        // current matrix
                        let items: Vec<f32> = op
                            .operands
                            .get(..6)
                            .ok_or("failed to get position for text in pdf")?
                            .iter()
                            .map(|f| f.as_float().unwrap())
                            .collect();
                        self.state
                            .cm(items[0], items[1], items[2], items[3], items[4], items[5]);
                        Ok(())
                    }
                    "Tm" => {
                        // text matrix
                        let items: Vec<f32> = op
                            .operands
                            .get(..6)
                            .ok_or("failed to get position for text in pdf")?
                            .iter()
                            .map(|f| f.as_float().unwrap())
                            .collect();
                        self.state
                            .tm(items[0], items[1], items[2], items[3], items[4], items[5]);
                        Ok(())
                    }
                    "Td" => {
                        // transforms tm
                        let items: Vec<f32> = op
                            .operands
                            .get(..2)
                            .ok_or("failed to get position for text in pdf")?
                            .iter()
                            .map(|f| f.as_float().unwrap())
                            .collect();
                        self.state.td(items[0], items[1]);
                        Ok(())
                    }
                    "TD" => {
                        // just like Td, just sets leading to -ty
                        let items: Vec<f32> = op
                            .operands
                            .get(..2)
                            .ok_or("failed to get position for text in pdf")?
                            .iter()
                            .map(|f| f.as_float().unwrap())
                            .collect();
                        self.state.td_capital(items[0], items[1]);
                        Ok(())
                    }
                    "TL" => {
                        // sets leading
                        self.state.tl(op.operands[0].as_float().unwrap());
                        Ok(())
                    }
                    "T*" => {
                        // applies leading
                        self.state.t_star();
                        Ok(())
                    }
                    "sc" | "rg" => {
                        // sets color
                        if let Some(items) = op.operands.get(..3) {
                            let r = items[0].as_float()?;
                            let g = items[1].as_float()?;
                            let b = items[2].as_float()?;
                            if r != 0.0 || g != 0.0 || b != 0.0 {
                                let color = rgb_to_hex(
                                    items[0].as_float()?,
                                    items[1].as_float()?,
                                    items[2].as_float()?,
                                );
                                current_element.color = Some(color);
                            }
                        }
                        Ok(())
                    }
                    "SC" | "RG" => {
                        // mostly happens after underlined
                        if let Some(last) = elements.last_mut() {
                            match last {
                                PdfUnit::Text(pdf_text) => pdf_text.underlined = true,
                                PdfUnit::Line(_) => {}
                            };
                        }
                        Ok(())
                    }
                    "m" => {
                        let items: Vec<f32> = op
                            .operands
                            .get(..2)
                            .ok_or("failed to get position for line in pdf")?
                            .iter()
                            .map(|f| f.as_float().unwrap())
                            .collect();
                        self.state.m(items[0], items[1]);
                        Ok(())
                    }
                    "l" => {
                        let items: Vec<f32> = op
                            .operands
                            .get(..2)
                            .ok_or("failed to get position for line in pdf")?
                            .iter()
                            .map(|f| f.as_float().unwrap())
                            .collect();
                        let (from, to) = self.state.l((items[0], items[1]));
                        let line = PdfLine { from, to };
                        elements.push(PdfUnit::Line(line));
                        Ok(())
                    }
                    "S" | "s" => {
                        if !pending_lines.is_empty() {
                            elements.extend(take(&mut pending_lines));
                        }
                        Ok(())
                    }
                    "F" | "f" | "f*" => {
                        pending_lines.clear();
                        Ok(())
                    }
                    "re" => {
                        let items: Vec<f32> = op
                            .operands
                            .get(..4)
                            .ok_or("failed to get position for line in pdf")?
                            .iter()
                            .map(|f| f.as_float().unwrap())
                            .collect();

                        let x = items[0];
                        let y = items[1];
                        let width = items[2];
                        let height = items[3];

                        let point1 = (x, y); // bottom-left corner
                        let point2 = (x + width, y); // bottom-right corner
                        let point3 = (x + width, y + height); // top-right corner
                        let point4 = (x, y + height); // top-left corner

                        let lines = vec![
                            self.state.re(point1, point2), // Line 1: bottom-left to bottom-right
                            self.state.re(point3, point3), // Line 2: bottom-right to top-right
                            self.state.re(point3, point4), // Line 3: top-right to top-left
                            self.state.re(point4, point1), // Line 4: top-left to bottom-left
                        ];
                        pending_lines = lines
                            .iter()
                            .map(|line| {
                                PdfUnit::Line(PdfLine {
                                    from: line.0,
                                    to: line.1,
                                })
                            })
                            .collect();

                        if is_fake_line(&pending_lines) {
                            elements.extend(take(&mut pending_lines));
                        }

                        Ok(())
                    }
                    "BDC" => {
                        // tag with text content / stream
                        //BDC: [/Span, <</ActualText (��T)>>]
                        //rest i don't thik we should care
                        match &op.operands[1] {
                            Object::Dictionary(dictionary) => {
                                let obj = dictionary.get(b"ActualText")?;
                                let text = self.extract_text_from_obj(obj);
                                let (x, y) = self.state.current_position();
                                current_element.text = text;
                                current_element.x = x;
                                current_element.y = y;
                                elements.push(PdfUnit::Text(take(&mut current_element)));
                            }
                            _ => {}
                        }
                        Ok(())
                    }
                    _ => {
                        // eprintln!("didnt hanlde {} with {:?}", op.operator, op.operands);
                        Ok(())
                    }
                }
            })
            .collect();
        Ok(elements)
    }

    fn get_current_encoding(&self) -> Option<&Encoding<'_>> {
        match self.encodings.get(&self.current_font_alias) {
            Some(en) => Some(en),
            None => {
                if let Some(enc) = &self.child_encodings {
                    let encoding = enc.get(&self.current_font_alias)?;
                    return Some(encoding);
                }
                None
            }
        }
    }

    fn extract_text_from_objs(&self, objs: &[Object]) -> String {
        let mut text = String::new();
        for obj in objs {
            text.push_str(&self.extract_text_from_obj(obj));
        }
        text
    }

    fn extract_bytes_from_obj(obj: &Object) -> Vec<u8> {
        match obj {
            Object::String(bytes, _) | Object::Name(bytes) => bytes.clone(),
            Object::Array(nested) => PdfPage::extract_bytes_from_objs(nested),
            _ => Vec::new(),
        }
    }

    fn extract_bytes_from_objs(objs: &[Object]) -> Vec<u8> {
        let mut text = Vec::new();
        for obj in objs {
            text.extend_from_slice(&PdfPage::extract_bytes_from_obj(obj));
        }
        text
    }

    fn extract_text_from_obj(&self, obj: &Object) -> String {
        let bytes = PdfPage::extract_bytes_from_obj(obj);

        // encoding from font
        if let Some(s) = self
            .get_current_encoding()
            .and_then(|encoding| Document::decode_text(encoding, &bytes).ok())
        {
            return s;
        }

        // if utf16
        if bytes.len() >= 2 {
            match (bytes[0], bytes[1]) {
                (0xFE, 0xFF) => {
                    // UTF-16BE
                    let u16s: Vec<u16> = bytes[2..]
                        .chunks(2)
                        .filter_map(|chunk| {
                            chunk.get(1).map(|&b1| u16::from_be_bytes([chunk[0], b1]))
                        })
                        .collect();
                    return String::from_utf16_lossy(&u16s);
                }
                (0xFF, 0xFE) => {
                    // UTF-16LE
                    let u16s: Vec<u16> = bytes[2..]
                        .chunks(2)
                        .filter_map(|chunk| {
                            chunk.get(1).map(|&b1| u16::from_le_bytes([chunk[0], b1]))
                        })
                        .collect();
                    return String::from_utf16_lossy(&u16s);
                }
                _ => {}
            }
        }

        // fallback to utf8
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

fn rgb_to_hex(r: f32, g: f32, b: f32) -> String {
    let r = (r * 255.0).round() as u8;
    let g = (g * 255.0).round() as u8;
    let b = (b * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}
