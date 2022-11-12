# specfmt
Like rustfmt and clang-format, but for web specs

It is expected that this tool be used when developing web specifications, such
as the Bikeshed specs that [WHATWG](https://github.com/WHATWG) works on, or even
the [HTML Standard](https://github.com/whatwg/html) (which uses a different
build system, but that doesn't matter for the purposes of this tool).

`specfmt` contains adapted and sometimes fixed algorithms from [the original
rewrapper](https://github.com/domenic/rewrapper), ported to Rust.

# Install

With Cargo installed, run:

```sh
$ cargo install specfmt
```

To install Cargo (the Rust package manager) follow [these
instructions](https://doc.rust-lang.org/cargo/getting-started/installation.html).

# Usage

You can format a web specification `file` by running:

```sh
$ specfmt [file]
```

Note that `file` is optional if you're inside the spec directory: `specfmt` will
try and find the unique `*.bs` file in the current directory, or `source` (for
[whatwg/html](https://github.com/whatwg/html)).

By default, `specfmt` will:
 - Wrap lines to 100 cols
 - Prevent you from formatting a spec with uncommitted changes
 - Scope its reformatting to changes in the current spec branch

To override any of this behavior, run `specfmt --help` to see additional command
line flags that you can pass in.
