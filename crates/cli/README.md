# Cli

## Testing

You can test your changes to the `cli` crate by first building the main zzz binary:

```
cargo build -p zzz
```

And then building and running the `cli` crate with the following parameters:

```
 cargo run -p cli -- --zzz ./target/debug/zzz.exe
```
