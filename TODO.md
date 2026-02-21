# Markdownify:

- fix the build tree to not include dups from the main cli, and also have a common root correctly, for instance:
  tar:

├── sample-1/
│ ├── sample-1.webp
│ ├── sample-1_1.webp
│ ├── sample-5 (1).jpg
│ ├── sample-5.webp

and a zip:

├── \_\_MACOSX/
│ ├── sample-1/
│ │ ├── .\_sample-1.webp
│ │ ├── .\_sample-1_1.webp
│ │ ├── .\_sample-5 (1).jpg
│ │ ├── .\_sample-5.webp
├── sample-1/
│ ├── sample-1.webp
│ ├── sample-1_1.webp
│ ├── sample-5 (1).jpg
│ ├── sample-5.webp

the tar has the base root included, the zip does not.

- add documention for the methods them selfs.
- runs tests on all the types, everything has changed.
- add a formatter in the lib method.

# Rasteroid:

- look at that.

# Mcat:

- make clap use the derive.
- change the entire create to use anyhow and tracing.
- make a seperate crate for viewing pdf, no longer apart of markdownify
- break the markdown viewer into another seperate crate.
- check overall code quality.
- change all deps to live in the workspace, the others import from there.
- solve all the github issues
