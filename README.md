# 🌍 Vakfu

**Vakfu** is an open-source Rust implementation of a map renderer for Wakfu, powered by the [Bevy](https://bevyengine.org/) game engine and the [byte](https://github.com/jac3km4/byte) crate for parsing game assets.

![Vakfu Screenshot](https://i.imgur.com/dx6mHVB.jpg)

---

## 🛠️ Building

This project requires [Rust and Cargo](https://rustup.rs/) to build.

### Dependencies

If you are building on Ubuntu or a similar Linux distribution, you will need to install several system dependencies for the Bevy engine. You can do this with:

```bash
sudo apt-get install pkg-config libwayland-dev libxkbcommon-dev libasound2-dev libudev-dev
```

### Compiling

Once everything is in place, you can build the project by running:

```bash
cargo build --release
```

---

## 🚀 Using

To run Vakfu, you need to point it to your local installation of Wakfu and specify a map ID.

```bash
cargo run --release -- --path /home/user/games/Ankama/Wakfu --map 999
```

Alternatively, if you've built the executable:

```bash
./target/release/vakfu --path /home/user/games/Ankama/Wakfu --map 999
```

### ⚠️ Disclaimer

This project **does not include any authored assets**. In order to run it, you must obtain a copy of the game *Wakfu*, created and owned by [Ankama Games](https://www.ankama.com/en/games/wakfu).

---

## 📄 License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
