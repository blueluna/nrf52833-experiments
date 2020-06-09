# nRF52833-DK experiments

Wrote this app when I built the nrf52833-pac and nrf52833-hal to test some
subsystems. The board used is the Nordic nRF52833-DK.

## Run

You can use `cargo embed` to transfer the examples to the board and run them.

Just enter the examples directory and use `cargo embed`.

```
$ cd nrf52833-dk
$ cargo embed --example blinky
```
