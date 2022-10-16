# specfmt
Like rustfmt and clang-format, but for web specs

Contains algorithms adapted and sometimes fixed algorithms from [the original
rewrapper](https://github.com/domenic/rewrapper), ported to Rust.

Usage:

```
specfmt [file] [--wrap=column_length]
```

or

```
cargo run -- [file] [--wrap=column_length]
```

It is expected that this tool be used when developing web specifications, such
as the Bikeshed specs that [WHATWG](https://github.com/WHATWG) works on, or even
the [HTML Standard](https://github.com/whatwg/html) (which uses a different
build system, but that doesn't matter for the purposes of this tool).
