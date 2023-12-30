# nRF52833-DK experiments

Wrote this app when I built the nrf52833-pac and nrf52833-hal to test some
subsystems. The board used is the Nordic nRF52833-DK.

Later on I added some examples for BBC micro:bit v2.

## Run

Install `probe-run` to run the examples.

```
$ cargo install probe-run
```

Enter the directory for the board and run the example.
```
$ cd microbit
$ DEFMT_LOG=info cargo run --example microbit-ccmstar
```
