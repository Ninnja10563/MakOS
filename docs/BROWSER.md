# MakOS Browser

Status: native bounded Browser runs as an isolated AArch64 EL0 process and
fetches HTTP pages through guest-owned virtio-net, DHCP, ARP, IPv4, UDP/DNS,
and TCP. No browser mock, host proxy, WebView, Chromium wrapper, or insecure
HTTPS downgrade is used.

## Implemented core

`kernel/src/browser.rs` is allocation-free and independent from kernel globals.
It provides:

- strict `http://` / `https://` URL parsing, ports, IPv6 host literals, fragment
  removal, relative URL resolution, dot-segment removal, and CRLF/control/host
  validation;
- bounded HTTP/1.1 GET construction using origin-form request targets;
- HTTP/1.0 and HTTP/1.1 response parsing, 16 KiB/64-header limits, duplicate
  `Content-Length` validation, transfer-length ambiguity rejection, chunked
  decoding, content type, redirect location, and status metadata;
- streaming-style HTML-to-readable-text extraction for headings, paragraphs,
  lists, preformatted text, entities, links, title, hidden head/style/script
  content, fixed-capacity output, and hard wrapping. Every emitted layout line
  is no wider than requested viewport;
- fixed-capacity navigation history with back/forward and forward-history
  invalidation after branched navigation.

`user/aarch64_browser.c` is a freestanding native EL0 app, not hosted UI. It
implements native Browser chrome, editable address, back/forward, scroll,
resize-driven reflow, DHCP DNS lookup, UDP DNS parsing, TCP sockets, bounded
HTTP receive, strict message framing, chunk decoding, HTML readable-text
rendering, and surface event loop. `https://` is visibly rejected until
certificate-verified TLS exists.

## Limits and security invariants

- URL: 1024 bytes in core; 512 bytes in EL0 UI. Userinfo forbidden.
- HTTP header: 16 KiB, 64 fields. Conflicting length and
  `Transfer-Encoding` + `Content-Length` are rejected.
- Native app response: 64 KiB; rendered text: 32 KiB; DNS packet: 512 bytes.
- HTML tokenizer bounds a tag scan to 512 bytes and never executes script.
- Layout hard-splits long tokens, preventing text outside content panel.
- Only `identity` content encoding is requested. gzip/Brotli unsupported.
- TLS, CSS, DOM mutation, images, JavaScript, cookies, downloads, cache,
  accessibility tree, and certificate store remain future work.

## Native AArch64 integration

- Build embeds a standalone `aarch64-browser.elf`; authenticated shell launches
  it with process selector 1. Browser gets separate page tables, PID, stack,
  sockets, surface, and role credentials limited to graphics/network/input.
- Browser owns stable compositor slot 5. Per-surface key/pointer/wheel/resize/close
  events prevent focused Browser input reaching Terminal. Close hides retained
  state; Start/taskbar reopen it.
- AArch64 SVC 47-51 provides generation-tagged, owner-checked UDP/TCP sockets.
  Process reaping closes leaked sockets. SVC 59 draws clipped owner-checked
  text, SVC 60 reads 28-byte surface events without blocking EL1, and SVC 61
  copies DHCP IPv4/gateway/DNS state.
- `aarch64_virtio_net.rs` drives modern virtio-mmio RX/TX queues directly.
  `aarch64_net_wire.rs` builds and validates Ethernet, ARP, IPv4, UDP, TCP, and
  DHCP packets including IPv4/UDP/TCP checksums. QEMU user networking supplies
  an ordinary virtual Ethernet segment; no HTTP proxy exists.
- Startup fetch proves DNS, TCP, HTTP parsing, and native surface rendering,
  then Browser hides in background so Terminal keeps focus. Start reopens the
  retained fetched page/history.

## Remaining browser work

- Add certificate-verified TLS 1.2/1.3 plus trust store before enabling HTTPS.
  Port 443 remains blocked rather than cleartext-downgraded.
- CSS layout, images, JavaScript, cookies, downloads, cache, accessibility tree,
  multiple tabs, and production-grade TCP congestion/retransmission remain.
- Expand deterministic UI automation for literal URL entry, chunked fixtures,
  back/forward, resize reflow, and pixel-boundary assertions. Start-menu
  retained reopen and titlebar close are boot-tested now.

## Verification

Run:

```sh
scripts/test_browser_core.sh
```

This executes parser/layout/history and Ethernet/IP wire tests, compiles EL0
source with strict warnings, rejects unresolved symbols, links static AArch64
ELF, and verifies its architecture. `make test-aarch64` additionally boots with
HVF/TCG, proves DHCP/ARP plus Browser DNS/TCP/HTTP/render markers, and checks UI.
