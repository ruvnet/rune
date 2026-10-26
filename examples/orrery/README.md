# Rune Evidence Orrery

A self contained architecture visualization with embedded WebAssembly, JavaScript, CSS and inline SVG. No install, build step or external asset is required.

## Run

Open index.html in Safari, Chrome or Firefox. For a local HTTP preview, run this command from the repository root:

```sh
python3 -m http.server 8080
```

Visit http://localhost:8080/examples/orrery/.

Attachment viewers may disable JavaScript. If the controls are inactive inside a file preview, open the file in a full browser or use the local HTTP preview.

## Controls

| Control | Behavior |
| --- | --- |
| Demo mode / P | Start or exit the seven scene guided tour |
| Previous / next / arrow keys | Navigate the demo scenes |
| Space / Pause | Pause or resume the animation and demo clock |
| Escape | Exit the demo |
| Loop | Repeat the tour; disable to stop after the last scene |
| 1 through 6 | Select an architecture node |
| B | Run the illustrative build cycle |
| D / Debug | Open diagnostics |
| Fullscreen | Request browser fullscreen where supported |
| Export SVG | Download the current SVG snapshot |
| Speed | Adjust simulation time and demo speed |

The tour uses seven seconds per scene at normal speed, for about 49 seconds per loop. Reduced motion preferences disable camera movement and decorative motion; the visualization initially pauses. An explicit Demo mode action starts the tour.

## Evidence boundary

All workflow and telemetry data are illustrative. The hardware selector does not compile or execute on the selected target.

The embedded WASM function calculates particle phase using time, speed and offset. It is a visualization kernel, not Rune compiler integration or a RuVector runtime. JavaScript provides an equivalent fallback when WASM is unavailable.

Node descriptions preserve the README distinction between implemented Phase 1 commands, validated fixture adapters and proposed integrations. There are no API calls, model requests, credentials or repository execution.

## Browser console

```js
await runeDebug.selfTest()
runeDebug.snapshot()
runeDebug.demo.start()
runeDebug.demo.seek(3)
runeDebug.demo.duration(7) // allowed: 2 through 30 seconds
runeDebug.demo.stop()
```

The self test checks three known phase calculations and the presence of 144 particles. It does not establish compiler correctness or target performance.

## Validation

Verified in local Chromium: WASM phase calculations, JS fallback, all demo navigation controls, pause, loop completion, single play completion, build cycle completion, node selection, SVG download and a 390 px mobile layout without horizontal overflow. No page errors were observed.

Safari and physical iOS device behavior have not been verified. Fullscreen and clipboard depend on browser support and permissions.

Acceptance check: start Demo mode, confirm the title changes after seven simulation seconds, pause and confirm the clock stops, then run `await runeDebug.selfTest()` and confirm `pass: true`.
