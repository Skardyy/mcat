<div align="center">

# Mcat

<img src="https://i.imgur.com/qSSM6Iy.png" width="128"/>

Parse, Convert and Preview files  
***In your Terminal***

![Downloads](https://img.shields.io/crates/d/mcat?style=for-the-badge) ![Version](https://img.shields.io/crates/v/mcat?style=for-the-badge)  

[Installation](#installation) • [Examples](#example-usage) • [CHANGELOG](./CHANGELOG.md)

![mcat_demo](https://github.com/Skardyy/assets/blob/main/mcat_opt.gif)
</div>

## Installation

<details>
   <summary>From Source</summary>

```sh
cargo install mcat
```
or ~
```sh
git clone https://github.com/Skardyy/mcat
cd mcat
cargo install --path ./crates/core
```
</details>

<details>
   <summary>Prebuilt</summary>

follow the instructions at the [latest release](https://github.com/Skardyy/mcat/releases/latest)
</details>
<details>
   <summary>Homebrew (MacOS/Linux)</summary>

```sh
brew install Skardyy/mcat/mcat
```
</details>
<details>
   <summary>AUR (Arch linux)</summary>

```sh
yay -S mcat-bin
```
</details>
<details>
   <summary>Winget (Windows)</summary>

```sh
winget install skardyy.mcat
```
</details>

## How it works

![mcat-pipeline](https://github.com/user-attachments/assets/fbf4617d-453a-45e8-bbd5-5dfdac2b8086)

<details>
<summary>Advanced explanation</summary>
   
---


| Input |
|-------|

Inputs can be:
1. local file
2. url
3. bytes from stdin

The type of each input is inferred automatically, and it continues through the pipeline until it reaches the output format the user requested.

| In the pipeline |
|-----------------|

For example, if the user runs:

```
mcat file.docx file.pdf -o inline
```

`mcat` will:
- Convert both `file.docx` and `file.pdf` into a single Markdown file
- Convert that Markdown into HTML
- Convert the HTML into an image
- Convert the image into an inline terminal image and print it

You can also start from the middle of the pipeline.  
For example:

```
mcat file.html -o image > image.png
```

This starts at an HTML file and directly converts it into a PNG image.
   
| Explanation of the blocks |  
|---------------------------|

* **`Markdown`** - set when `-o md` or when the stdout isn't the terminal (piped)  

* **`Pretty Terminal`** is markdown with ANSI formatting, and is the **default** for any non video / image file. (the `-c` flag forces it)

* **`HTML`** set when `-o html` -- only works for non image / video files  

* **`Static Image`** set when `-o image` and gives an image  

* **`Interactive Image`** set when `-o interactive` and launches an interactive view to zoom and pan the image in the terminal.  

* **`Inline Display`** set when `-o inline` or `-i` and prints the content as image in the terminal  

---
</details>


## Example Usage
```sh
#------------------------------------#
#  View a documents in the terminal  #
#------------------------------------#

mcat resume.pdf
mcat project.docx -t monokai           # With a different theme
mcat "https://realpdfs.com/file.pdf"   # From a url
cat file.pptx | mcat                   # From stdin
mcat .                                 # Select files interactively

#-----------------# 
#  Convert files  #
#-----------------#

mcat archive.zip > README.md           # Into Markdown
mcat f1.rs f2.rs -o html > index.html  # Into HTML
mcat index.html -o image > page.png    # Into image

#--------------------------#
#  View Images and Videos  #
#  in the terminal         #
#--------------------------#

mcat img.png                           # Image
mcat video.mp4                         # Video
mcat "https://giphy.com/gifs/..."      # From a URL
mcat README.md -i                      # Converts to image and then shows it
mcat ls                                # ls command with images
mcat massive_image.png -o interactive  # zoom and pan the image interactively in the terminal

#--------------------------#
#  What I use it most for  #
#--------------------------#

mcat ls                                # To find the image i was looking for
mcat . | scb                           # Selects files, concat them, and copy to clipboard ~ for AI prompts
mcat index.html -o image > save.png    # Render HTML into images
```

## Optional Dependencies
> Mcat will continue working without them
<details>
<summary><strong>Chromium (for rendering HTML to image)</strong></summary>

---
1. Available by default on most Windows machines via Microsoft Edge.
2. Also works with any installed Chrome, Edge, or Chromium.
3. You can install it manually via `mcat --fetch-chromium`
---
</details>

<details>
<summary><strong>pdftocairo/pdftoppm (for rendering PDF to image)</strong></summary>

---
1. Is included by default in most major distros
2. Windows users can install from [poppler-windows](https://github.com/oschwartz10612/poppler-windows)
3. If not installed, mcat will fallback into converting the PDF to Markdown and then screenshot using chromium
---
</details>

<details>
<summary><strong>FFmpeg (for videos)</strong></summary>

---
1. If it's already on your machine.
2. Otherwise, you can install it with `mcat --fetch-ffmpeg`
---
</details>

## Configuring
<details>
<summary><strong>Using Flags</strong></summary>

---
the main flags for configuring are:
* `--opts` for inline image printing
* `--ls-opts` for the ls command

run `mcat --help` for full detail, and other flags. 

---
</details>

<details>
<summary><strong>Using Environment Variables</strong></summary>

---
each variable mimicks its corresponding flag alternative.
* `MCAT_ENCODER`, Options: kitty,iterm,sixel,ascii. e.g. MCAT_ENCODER=kitty is the same as doing `--kitty`
* `MCAT_PAGER`, <str> the full command mcat will try to pipe into.
* `MCAT_THEME`, <str> same as the `--theme` flag
* `MCAT_INLINE_OPTS`, <str> same as the `--opts` flag
* `MCAT_LS_OPTS`, <str> same as the `--ls-opts` flag
* `MCAT_SILENT`, <bool> same as the `--silent` flag
* `MCAT_NO_LINENUMBERS`, <bool> same as the `--no-linenumbers` flag
* `MCAT_MD_IMAGE`, <bool> same as the `--no-images` flag
---
</details>


## Roadmap
- [ ] mcat.nvim: a neovim plugin to use mcat inside neovim

## License
MIT License

---

<div align=center>
   <p>Thanks to all contributors</p>   
   <img src="https://contributors-img.web.app/image?repo=skardyy/mcat" height="30"/>
   <br/>
   <br/>
   <p>Thanks to all sponsors</p>
   <a href="https://www.warp.dev/">
      <img height="70" src="https://github.com/user-attachments/assets/c21102f7-bab9-4344-a731-0cf6b341cab2">
   </a>
</div>
