# vakfu
Vulkan renderer for Wakfu

![Imgur](https://i.imgur.com/sqxzak2.png)

# building

This project requires [cargo](https://crates.io) to build.

All the dependencies are handled by cargo with the exception of Vulkan OS-specific libraries.
Instructions on how to sort them out can be found in the Setup section of the README in the [vulkano](https://github.com/vulkano-rs/vulkano) project.

Once everything is in place:
```bash
cargo build
```
