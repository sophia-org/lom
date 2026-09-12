# Third-party material

## Ironbar inspiration and adapted configuration/theme material

The Minimal arrangement and theme values are adapted from
JakeStanger/ironbar, inspected through sophia-org/ironbar at
`e2910c7fded664dd1f4560217a92ba2030051746` (`examples/minimal/`).
The workspace/clock/configuration source also informed behavior. Lom's Rust
implementation is original; it does not include ironbar's GTK implementation.
The adapted material retains this notice:

```text
MIT License

Copyright (c) 2022 Jake Stanger

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Bundled fonts

Unmodified DejaVu Sans Mono regular and bold, copied from this host's
`dejavu-fonts-ttf` package. The DejaVu/Bitstream notices and redistribution terms
are preserved in [assets/fonts/LICENSE](assets/fonts/LICENSE).

| Asset | SHA-256 |
| --- | --- |
| DejaVuSansMono.ttf | `b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582` |
| DejaVuSansMono-Bold.ttf | `bce60f1b4421acd9ea51ba6623d7024ecbe6817a953e3654df62a5e6bdf8f769` |

The sample tray symbols are glyphs from these fonts, not third-party icon images.

## Libraries

Xilem, Masonry, Vello, Parley and the remaining Cargo dependencies retain their
upstream licenses. Cargo.lock pins the resolved sources. Software rendering is
used solely by dev dependencies for deterministic image tests; production uses
Vello GPU.
