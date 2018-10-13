# vakfu
Vulkan renderer

![Imgur](https://i.imgur.com/RcOykz6.png)

# building

This project requires [cargo](https://crates.io) to build.

All the dependencies are handled by cargo with the exception of Vulkan OS-specific libraries.
Instructions on how to sort them out can be found in the Setup section of the README in the [vulkano](https://github.com/vulkano-rs/vulkano) project.

Once everything is in place:
```bash
cargo build
```

# using

Note that this project does not include **any** authored assets. In order to run it, you may get such assets by obtaining a copy of the game Wakfu, created by Ankama Games.
