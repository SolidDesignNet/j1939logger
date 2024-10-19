Simple RP1210 based J1939 CAN logger.

J1939 CAN logs and DBC display for RP1210 devices using multiple channels.
![image](https://github.com/SolidDesignNet/j1939logger/assets/1972001/d7596418-933e-428e-9f7e-9170cb49a768)

To use:
1. Select adapter (rp1210 adapter driver must be installed):
![image](https://github.com/SolidDesignNet/j1939logger/assets/1972001/402f00df-0211-40cf-b758-5937fe3bc75b)
2. That's all.

The log can be saved to another file, or copy and paste to a text editor.

The log uses the adpater to decode the J1939 Transport Protocol.

Loading a DBC file will open another window which decodes the signals defined in the DBC file.  If the DBC file has incorrect source addresses defined (sometimes FEx is used as a placeholder), Action/Map Address... will allow you to change the SA for all signals with the wrong SA.  Copy and paste also works in this window.

To build using Win32 32 bit gnu toolchain:
1. Install MSYS2.
2. `pacman -S mingw-w64-i686-cmake mingw-w64-i686-make mingw-w64-i686-gcc curl tar git --needed`
3. Start msys2 mingw32 32 bit.
4. add `~/.cargo/bin` to your path
5. `cargo build`