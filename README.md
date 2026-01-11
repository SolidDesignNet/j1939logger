### J1939Logger
Simple RP1210, SLCAN, and SocketCAN based J1939 CAN logger.

J1939 CAN logs and DBC display.
![image](https://github.com/SolidDesignNet/j1939logger/assets/1972001/d7596418-933e-428e-9f7e-9170cb49a768)

### Usage 

1. Select adapter (RP1210 adapter driver must be installed or SLCAN device using a serial connection or SocketCAN device already configured using the Linux network stack):
![image](https://github.com/SolidDesignNet/j1939logger/assets/1972001/402f00df-0211-40cf-b758-5937fe3bc75b)
2. That's all.

The log can be saved to another file or copy and paste to a text editor.

The log uses the adapter to decode the J1939 Transport Protocol if available, but will decode TP in the application for adapters that do not (like SLCAN).

Loading a DBC file will open another window which decodes the signals defined in the DBC file.  If the DBC file has incorrect source addresses defined (sometimes FEx is used as a placeholder), Action/Map Address... will allow you to change the SA for all signals with the wrong SA.  Copy and paste also works in this window.

I use the SLCAN adapter: https://www.amazon.com/dp/B0CY9R7PBP

I have also successfully used NEXIQ, Noregon, Vector, and Peak adapters.

### Goal
CAN logging with very light analysis and scripting.  This needs to be simple, not a replacemnent for CANAlyzer.

### For Developers:

I develop on Linux and cross compile for Windows. https://github.com/cross-rs/cross?tab=readme-ov-file#installation 

* for Linux: `cargo build --release`
* ~~for Windows 64 bit: `cross build --target x86_64-pc-windows-gnu --release`~~ No longer works for me. I had to install the x86_64-pc-windows-gnu toochain and the native ubuntu libraries g++-mingw-w64 and gcc-mingw-w64-x86-64, then `cargo build --release --target x86_64-pc-windows-gnu`
* ~~for Windows 32 bit: `cross build --target i686-pc-windows-gnu --release`~~ No longer works for me. I'm not pursuing this anymore.

_I no longer do this and will not support this method any more._ To compile on Windows for 32 bit (to support old RP1210 adapters). To build using Win32 32 bit gnu toolchain:

1. Install MSYS2.
2. Update MSYS2 with `pacman -S mingw-w64-i686-cmake mingw-w64-i686-make mingw-w64-i686-gcc curl tar git --needed`
3. Start msys2 mingw32 32 bit.
4. add `~/.cargo/bin` to your path
5. `cargo run`
