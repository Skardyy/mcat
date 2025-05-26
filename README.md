<div align="center" markdown="1">
   <sup>Special thanks to:</sup>
   <br>
   <br>
   <a href="https://www.warp.dev/mcat">
      <img alt="Warp sponsorship" width="400" src="https://github.com/user-attachments/assets/c21102f7-bab9-4344-a731-0cf6b341cab2">
   </a>

### [Warp, the intelligent terminal for developers](https://www.warp.dev/mcat)
[Available for MacOS, Linux, & Windows](https://www.warp.dev/mcat)<br>

</div>
<hr>

<div align="center">
  
<img src="https://i.imgur.com/qSSM6Iy.png" width="128"/>


# Mcat
![Downloads](https://img.shields.io/crates/d/mcat?style=for-the-badge) ![Version](https://img.shields.io/crates/v/mcat?style=for-the-badge)  

[Installation](#%EF%B8%8F-installation) • [Examples](#%EF%B8%8F-example-usage) • [CHANGELOG](./CHANGELOG.md)

![mcat_demo](https://github.com/Skardyy/assets/blob/main/mcat_opt.gif)
</div>

## ✨ Features
* 📄 **File to Markdown/HTML**  
  Convert structured content like CSVs, directories, and rich document formats (e.g., DOCX) into clean Markdown/HTML.
  
* 🏞️ **Markdown/HTML to Image**  
  Render Markdown or HTML files into images.
  
* 🖼️ **Inline Image/Videos**  
  Display images/videos *inside* your terminal using protocols like Kitty, iTerm, or Sixel.
  
* 🌐 **Handles URLs too!**  
  View Images/Videos/Document from a URL in your terminal (or save them)

* 🔗 **Concatenate Images and Video too!**  
  Concatenate videos of the same format (time concat)
  and Concatenate images by stacking them horizontal or vertical(default)
  
* 💃🏻 **Automatic Styling for HTML**  
  automatically inject styles into the HTML to make the image cooler!

## ⬇️ Installation
```sh
cargo install mcat
```
or ~
```sh
git clone https://github.com/Skardyy/mcat
cd mcat
cargo install --path ./crates/core
```
or prebuilt from the [latest release](https://github.com/Skardyy/mcat/releases/latest)

## 🏋️ Example Usage
```sh
# View a document at the terminal
mcat resume.pdf

# Or from a url!
mcat "https://somewebite.com/file.pdf"

# Or HTML
mcat project.docx -o html

# ls, you can list a dir
mcat ls

# Show a document inline as an image
mcat readme.md -i

# Or HTML!
mcat index.html -i

# Just save?
mcat index.html -o image > page.png

# Show a document as an image inline with a different theme
mcat main.rs another.rs -it monokai

# Render a document to an image and save it
mcat readme.md -o image > img.png

# Show media inline in your terminal
mcat diagram.png
mcat video.mp4

# From a url
mcat "https://giphy.com/gifs/..."

# Images too!
mcat "https://website/images/..."

# From stdin?
mcat < somefile.png

# Concatenate documents and turn them into an image
mcat document.docx presentation.odt table.xlsx archive.zip -o image > all.png

# Or just select interactively and copy to clipboard (for ai prompts)
# Replace scb with a command from your os
mcat directory | scb

# Concatenate images (stacks vertical)
mcat someimage.png anotherimage.bmp

# Or save it (stacks horizontal)
mcat someimage.png anotherimage.bmp --hori -o image > save.png

# Concatenate videos (must be same format: codec,audio..)
mcat part1.mp4 anothervideo.mp4 -o video > save.mp4
```

## ⚙️ Supported Formats
| Input Type | Output Options |
|---|---|
| DOCX, PDF, CSV, ODT, PPTX, and more.. | Markdown, HTML, Image, Inline |
| Markdown | Pretty terminal formatting |
| Markdown / HTML | Image, Inline Image |
| Images, Videos | Inline Display |
| URLs | Fetch + Everything above! |

## 🛐 Dependencies
Mcat tries to have as little dependencies as possible.
#### chromium (for rendering HTML to image):
1. exists on every windows machine through msedge. and other machines that have chrome/msedge/chromium
2. can be installed by doing `mcat --fetch-chromium`
#### ffmpeg (for videos)
1. if your machine has it 🫠.
2. can be installed by doing `mcat --fetch-ffmpeg`

## 🆘 Help
```txt
mcat --help
Usage: mcat [OPTIONS] [input]...

Arguments:
  [input]...  file / dir / url

Options:
  -o, --output <output>                  the format to output [possible values: html, md, image, video, inline]
  -t, --theme <theme>                    the theme to use [default: dark] [possible values: dark, light, catppuccin, nord, monokai, dracula, gruvbox, one_dark, solarized, tokyo_night]
  -s, --style-html                       add style to html too (when html is the output)
      --delete-images                    deletes all the images, even ones that are not in the scrollview.. currently only works in kitty, and will be extended..
  -a, --hidden                           include hidden files
      --kitty                            makes the inline image encoded to kitty
      --iterm                            makes the inline image encoded to iterm
      --sixel                            makes the inline image encoded to sixel
      --ascii                            makes the inline image encoded to ascii
  -i                                     shortcut for putting --output inline
      --hori                             concat images horizontal instead of vertical
      --inline-options <inline-options>  options for the --output inline
                                         *  center=<bool>
                                         *  width=<string> [only for images]
                                         *  height=<string> [only for images]
                                         *  scale=<f32>
                                         *  spx=<string>
                                         *  sc=<string>
                                         *  inline=<bool>
                                         *  zoom=<usize> [only for images]
                                         *  x=<int> [only for images]
                                         *  y=<int> [only for images]
                                         *  exmp: --inline-options 'center=false,width=80%,height=20c,inline=true,scale=0.5,spx=1920x1080,sc=100x20,zoom=2,x=16,y=8'
      --report                           reports image / video dimensions when drawing images. along with reporting more info when not drawing images
      --silent                           removes loading bars
      --fetch-chromium                   download and prepare chromium
      --fetch-ffmpeg                     download and prepare ffmpeg
      --fetch-clean                      Clean up the local binaries
      --generate <generate-completions>  Generate shell completions [possible values: bash, zsh, fish, powershell]
  -h, --help                             Print help
  -V, --version                          Print version
```

## 🚧 Roadmap
- [ ] mcat.nvim: a neovim plugin to use mcat inside neovim

## 📎 License
MIT License
