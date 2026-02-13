## Src

- HTML to image now is slightly faster, and produces better images for small content
- `--opts` flag now supports both scalex and scaley
- fixed an issue where color would persist after a simple code block in the **markdown viewer**

## V0.4.6

- **markdown viewer** now renders YAML headers in a box, but now disabled by default - to show the header you supply the `--header` flag
- now **.url** files are also supported for image preview
- now **.exe** files are also supported for image preview
- now **.lnk** files are also supported for image preview
- the **ls command** now supports hyprlinks when supplied with the `--hyprlink` flag, or when the `MCAT_HYPRLINK` env var is set to either true or 1
- **ls command** now supports different sorting methods via the `--sort` flag, also with the `--reverse` flag

## V0.4.5

- **markdown viewer** now supports `<figure>` and `<figcaption>` HTML elements
- added macOS x86_64 build to the release
- fixed an issue in the `ls` command where unicode characters that are more then a single byte could cause a panick

## V0.4.4

- **interactive viewer** now supports albums - passing multiple images with `-o interactive` can now be viewed as an album (n/p to move between images)
- **interactive viewer** now automatically treats pdf/latex/typst as albums so you can view multiple pages.
- **markdown viewer** now handles multi line links better (link images too)
- **markdown viewer** now creates clickable links
- fixed an issue that stopped pdf files from being used with `-o interactive`
- fixed an issue that stopped certain files from being used with `-o image`

## V0.4.2

- now latex/typst files can also be converted into images / inline images
- **markdown viewer** now handles local images too! (#24) by @Alb-O
- **markdown viewer** now also handles footnotes

## V0.4.1

- fixed a cleanup issue that causes the **markdown viewer** to take longer when images are included.

## V0.4.0

- **markdown viewer** now parses some HTML!, including `align=center` attributes on some elements
- **markdown viewer** now includes Images! - can be modified using `--md-image all/small/none/auto` the default is "auto"
- **markdown viewer** improved - better formatting for some elements and now indents content under headers.
- fixed an issue in the **markdown viewer** when certain styles would reset others

## V0.3.8

- added autumn and spring themes
- improved the **markdown viewer** (prettier, comments HTML, better line wrapping in code blocks)
- HTML will now be treated as markdown when no output is specified -- allows for syntax highlighted code blocks instead of just printing it back.
- now removes the background color when converting HTML to image

## V0.3.6

- added ayu, ayu_mirage, synthwave, material, rose_pine, kanagawa, vscode, everforest and github themes!
- **markdown viewer** now uses the theme colors and not terminal colors
- improved the **markdown viewer** - less clutter
- improved the pdf to **markdown parser** -- now maintain layout and draws lines, in the cost of being more text then markdown.
- screenshots of HTML/documents no longer says the filename / arg is too long

## V0.3.4

- now allows selection from the interactive selector along with other inputs
- now converts PDF to images using pdftoppm/pdftocairo (if not installed fallback to markdown parsing)
- optimized build time
- fixed double linebreaks problem in the markdown viewer
- fixed codeblocks inside indented blocks being wider then the screen (markdown viewer).
- fixed an inconsistent box drawing character in codeblock (markdown viewer)
- fixed weird spacing when turning HTML to image in linux

## V0.3.3

- changed the colors in the interactive selector
- added line wrapping for the markdown viewer -- doesn't skip lines in less now

## V0.3.2

- fixed a bug where the names of files in the ls command won't show in windows
- made the interactive selector prettier -- now with icons, colors and more ANSI formatting
- added `--paging, -p, -P` flags to disable / enable paging forcefully
- added `--pager` flag and MCAT_PAGER env, to modify the pager used
- added `--color -c -C` flags to enable / disable ANSI formatting forcefully

## V0.3.1

- fixed an issue that tmux passthrough won't be enabled on the ls command
- made the interactive image viewer blink less ~ to none -- making it easier to the eye
- added a `--no-linenumber` flag to remove line numbers from the markdown viewer
- raw text from stdin now defaults to markdown instead of txt in the markdown viewer
- improved rendering of images in tmux by moving the cursor after the image
- now allows configuring things through env variables
- improved GP support auto detetion -- especially in tmux
- the ls command now combines images by row to fix bugs from quick image printing
- added `--ls-opts` flag, allowing users to configure the ls command
- the `--report` flag now shows more info
- fixed an issue where the interactive selector had special visible in windows
- ascii video play now doesn't blink

## V0.3.0

#### New Features:

- added -a --hidden flag for showing hidden files, along with making hidden files off by default.
- --pretty -p flag removed in favor of auto detecting if stdout is tty
- the pretty print of markdown is significantly improved
- now attempts to send text to a pager when the output is bigger then the screen and stdout is tty
- added catppuccin, nord, monokai, dracula, gruvbox, one_dark, solarized, tokyo_night themes!
- added `--generate` flag for generating shell completions for zsh/bash/fish/powershell
- kitty animation frames are stored in shm objects (writes the animation way faster, and less cpu power)
- added tmux support
- added kitty inline support; allows for having kitty images/animations be scrollable in apps like vim,tmux.
- added `-o interactive` mode to zoom & pan images for more detail

#### Fixes:

- fixed an issue where the zoom / pan aspect ratio would stay the same, making it difficult to see in some cases.
- fixed an issue in the ls command that would make the first item in a row up by 1 cell
- improved Iterm's graphic protocol support-detection
- fixed an issue that restricted rendering HTML into image directly
- fixed an issue where the process will quit when detecting symlink loop instead of just continuing

## V0.2.8

- adding an ls command
- adding parallelism for heavy operations

## V0.2.7

- bumping zip version because it was yanked

## V0.2.6

- adding ascii encoder for images and videos!
- sixel terminals can now use the ascii encoder to view videos too!
- fixed a bug in markdownify pdf parser where certain text would appear twice 1 after the other
- added the --report flag to query info
- added loading bars for long operations
- added --silent flag to remove the loading bars

## V0.2.5

- now expands ~
- naming files better when concatenating
- adding more filters to the recursive walk of dirs

## V0.2.4

- more fixes to the PDF parser, along with attempts to context headers
- improving the -p --pretty flag

## V0.2.3

- fixing issues with the PDF parser, along with improving it.

## V0.2.1

- fixed an issue in the interactive dir selector, where branches with the same name will be confused
- fixed an issue with the sixel encoder failing if the image isn't a png in some cases

## V0.2.0

- improved the PDF parser.
- now accepts from stdin (introspects the file type on its own.)
- handles URLs way better now, with more support for mime types. (including documents like PDF, ZIP, et..)

## V0.1.52

- auto download is now an option through the flags --fetch-chormium, --fetch--ffmpeg. and also adding --fetch-clean to remove after them.
- added a --output pretty and -p for printing markdown as pretty text in the terminal

## V0.1.51

- fixed issue with zombie process of chromium
- removed the --raw flag (chromium sandbox should suffice)

## V0.1.5

- now says when a path doesn't exists instead of saying Failed Reading
- adding zoom, x, y in the inline-options (--inline-options "")

## V0.1.4

now closing kitty animations when interrupted mid way

## V0.1.3

removes feature that requires native-tls (for cross compile)

## V0.1.2

#### new features

- concatenate images (vertical or horizontal)
- concatenate videos (time based, must be same format)
- scale image while maintaining center via --inline-options "scale=<f32>"

#### improved

- text based concatenate

## V0.1.1

now accepts multi input:
mcat file.docx file.pptx file.odt ..

## V0.1.0

First Release
