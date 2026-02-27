# 1

- [ ] make the markdown viewer only trim spaces and newlines, fixes #51 probbs #56 too
- [ ] make better file type detection, for instance .ts file is treated as a video while it could be typescript file too. fixes #55
- [ ] add a width and height flags, #49, probbs need to rewrite the entire flag parsing since it got kinda messy.
- [ ] add musl build, probbs gonna make my own ci since i want more things in the artifacts. we'll add: license, bin, man, completions, checksum.

# 2

- [ ] add a better markdown formatter
- [ ] add tracing.
- [ ] use anyhow instead of dyn objs anywhere..

# 3

- [ ] make a github page doc, should include both mcat, markdownify and rasteroid.
- [ ] break the markdown viewer into a separate crate.
- [ ] make a new crate for pdf viewing, should not be treated as markdown.
