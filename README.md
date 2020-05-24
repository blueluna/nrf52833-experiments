# nRF52833-DK experiments

Wrote this app when I built the nrf52833-pac and nrf52833-hal to test some
subsystems. The board used is the Nordic nRF52833-DK.

## Debug

[JLinkGDBServer] from Segger is used to debug, see the `jlinkgdb` shell script
on how JLinkGDBServer is invoked.

Start the GDB server with `jlinkgdb`.

```
$ ./jlinkgdb
```

Then run the program

```
$ cargo run
```

cargo will use the run definition found in `.cargo/config` to launch `gdb` with
the `jlink.gdb` script file.
