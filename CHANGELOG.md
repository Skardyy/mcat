## V0.3.2 (Not Released)
- [ ] added a `-p --playback` flag for watching videos in the terminal with sound

## V0.3.1 (Not Released)
- [x] fixed an issue that tmux passthrough won't be enabled on the ls command
- [x] made the interactive image viewer blink less ~ to none -- making it easier to the eye
- [x] added a `--no-linenumber` flag to remove line numbers from the markdown viewer
- [x] raw text from stdin now defaults to markdown instead of txt in the markdown viewer
- [x] improved rendering of images in tmux by moving the cursor after the image
- [ ] added an optional config file, allowing users to have a config file instead of aliasing 
- [x] improved GP support auto detetion -- especially in tmux
- [x] the ls command now combines images by row to fix bugs from quick image printing
- [x] added `--ls-opts` flag, allowing users to configure the ls command
- [x] the `--report` flag now shows more info
- [x] fixed an issue where the interactive selector had special visible in windows
- [x] ascii video play now doesn't blink

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
* adding ascii encoder for images and videos!
* sixel terminals can now use the ascii encoder to view videos too!
* fixed a bug in markdownify pdf parser where certain text would appear twice 1 after the other
* added the --report flag to query info
* added loading bars for long operations
* added --silent flag to remove the loading bars

## V0.2.5
* now expands ~
* naming files better when concatenating
* adding more filters to the recursive walk of dirs

## V0.2.4
* more fixes to the PDF parser, along with attempts to context headers
* improving the -p --pretty flag

## V0.2.3
* fixing issues with the PDF parser, along with improving it.

## V0.2.1
* fixed an issue in the interactive dir selector, where branches with the same name will be confused
* fixed an issue with the sixel encoder failing if the image isn't a png in some cases

## V0.2.0
* improved the PDF parser.
* now accepts from stdin (introspects the file type on its own.)
* handles URLs way better now, with more support for mime types. (including documents like PDF, ZIP, et..)

## V0.1.52
* auto download is now an option through the flags --fetch-chormium, --fetch--ffmpeg. and also adding --fetch-clean to remove after them.
* added a --output pretty and -p for printing markdown as pretty text in the terminal

## V0.1.51
* fixed issue with zombie process of chromium
* removed the --raw flag (chromium sandbox should suffice)

## V0.1.5
* now says when a path doesn't exists instead of saying Failed Reading
* adding zoom, x, y in the inline-options (--inline-options "")

## V0.1.4
now closing kitty animations when interrupted mid way

## V0.1.3
removes feature that requires native-tls (for cross compile)

## V0.1.2
#### new features  
* concatenate images (vertical or horizontal)  
* concatenate videos (time based, must be same format)  
* scale image while maintaining center via --inline-options "scale=<f32>"
#### improved  
* text based concatenate

## V0.1.1
now accepts multi input:
mcat file.docx file.pptx file.odt ..

## V0.1.0
First Release
