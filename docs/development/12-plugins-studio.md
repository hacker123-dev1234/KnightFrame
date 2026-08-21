# 12 - DSH-compatible plugins and Plugin Studio

## 1. Goal and compatibility boundary

KnightFrame uses DeepSeek Harness (DSH) Cordis behavior as the compatibility contract: dependency injection, pending activation, deterministic teardown, intercept/isolate metadata, stable entry IDs, transactional reload, host/client halves, and inspectable contributions. It does not treat a JavaScript object ABI or `node:vm` as a cross-language or security boundary.

Compatibility has two separate layers:

1. Static `cordis.yml` compatibility parses and emits the DSH entry fields `id`, `name`, `config`, `group`, `disabled`, `inject`, `intercept`, and `isolate`. A row still has to name a real importable Cordis JavaScript module; DSH does not consume a KnightFrame manifest or declarative UI JSON from this file.
2. `KnightFrame Plugin Wire v1` carries the same lifecycle and service semantics over JSON-RPC 2.0 so Rust, JavaScript/TypeScript, Python, and other executable runtimes behave identically.

A non-JavaScript KnightFrame plugin uses the wire protocol in KnightFrame. It does not thereby become a DSH Host plugin. Native DLL loading is out of scope because Windows cannot reliably unload arbitrary native code or preserve a stable Rust ABI.

Studio's DSH preview uses DSH's real dynamic-package path instead: it exports the exact argument object for `cordis_define`, including a plain JavaScript `code.client` function body. The Host records that immutable Package, then `cordis_run` activates it through `@deepseek-ai/dsh-cordis-host-runner` and `@deepseek-ai/dsh-cordis-client-runner`. This route is process-local and a Client-bearing Package requires the DSH approval flow. It does not bridge KnightFrame Rust/Python/command tools into DSH.

## 2. Manifest

Every managed plugin has a UTF-8 JSON manifest:

```json
{
  "protocolVersion": "knightframe.plugin.v1",
  "id": "sample.counter",
  "name": "Counter",
  "version": "0.1.0",
  "runtime": "rust|node|python|command",
  "entry": "bin/counter.exe",
  "configSchema": { "type": "object" },
  "inject": ["workspace"],
  "provide": ["counter"],
  "intercept": {},
  "isolate": { "counter": true },
  "tools": [],
  "ui": [],
  "permissions": ["ui"]
}
```

IDs and versions are human-readable and are the only identifiers exposed to the model or ordinary UI. Internal content fingerprints may be used for update validation but never enter prompts.

## 3. Lifecycle and cleanup

The target KnightFrame runtime state machine is:

```text
Pending -> Loading -> Active -> Unloading -> Disposed
                    -> Failed
```

- Missing injected services keep a plugin in `Pending`; this is not a failure.
- Every tool, service, event listener, UI contribution, timer, child process, and temporary resource is registered in an effect ledger.
- Stop, disconnect, crash, or dependency loss drains the ledger in reverse registration order.
- Reload starts a candidate runner, validates its manifest and contributions, waits for health, atomically switches the contribution set, then stops the old runner. A failed candidate leaves the old version active.
- Event dispatch supports `emit`, `parallel`, `serial`, `bail`, and `waterfall`; waterfall stops when a handler does not return a next value.

These lifecycle bullets are an implementation contract, not a claim about the current phase. The current Rust module validates manifests, Cordis entries, wire frames, and Studio exports; it does not yet start plugin processes, resolve dependency recovery, or perform transactional reload.

## 4. Wire protocol

Transport is newline-delimited JSON-RPC 2.0 over managed stdio. Windows runners are created without a visible terminal and assigned to a Job Object so application shutdown or cancellation cannot leave child processes behind.

Only framing and schema validation are implemented in the current phase. Managed process launch and the request handlers below remain runtime work.

Host requests:

- `plugin.hello`
- `plugin.activate`
- `plugin.invoke`
- `plugin.event`
- `plugin.health`
- `plugin.stop`
- `plugin.dispose`

Plugin requests:

- `host.provide` / `host.unprovide`
- `host.tool.register` / `host.tool.unregister`
- `host.ui.register` / `host.ui.unregister`
- `host.event.on` / `host.event.off`
- `host.call`
- `host.log`

Requests and responses must be losslessly representable as JSON. Unknown methods, duplicate contribution IDs, non-JSON values, invalid state transitions, oversized frames, and calls after disposal fail with stable localized error keys.

## 5. UI contribution protocol

Cross-language plugins contribute declarative slot patches rather than React/Svelte objects. A patch names a known slot, component type, stable ID, props, and optional event command. Initial component types are `button`, `toggle`, `text`, `input`, `select`, `separator`, and `panel`.

Unknown slots or component types are rejected before activation. Trusted JavaScript client modules may later use the DSH lazy client-module bridge, but this optional path cannot be required by Rust or Python plugins.

For DSH previews, Studio exposes only the additive `shell.overlay`, `sidebar.footer.action`, and session-keyed `tool.view.cordis` slots. The tool view always exports with key `self`; DSH's Client guard rewrites that value to the active Plugin and Package identity. Studio does not offer `sidebar`, `sidebar.settings`, or `sidebar.workspaces`, because registering there replaces shipped navigation.

The declarative contribution JSON is KnightFrame adapter data, not a DSH-native protocol. Export embeds that validated data into generated plain JavaScript which returns a Client Cordis Plugin with `inject: ['slots']`, waits through `ctx.slots.inject`, registers through `ctx.slots.register`, renders with the evaluator-provided `React`, and owns CSS through `styles.insert`. It emits no `import`, `require`, JSX, browser timer, DOM-global, or network access. The exported `cordis.yml` is deliberately a valid empty top-level array with comments directing the user to `cordis_define`; it does not claim to activate the preview. The preview also carries the localized reason key `studio.dsh.dynamic_requires_cordis_define`. Inventing a package name in `cordis.yml` would produce a configuration DSH cannot load.

## 6. Plugin Studio

Plugin Studio is a native Tauri child window, never an external browser. It has three first-class views:

- Design embeds the packaged KnightFrame entry as the real host. For DSH it starts the locally built DSH Web profile as a hidden, isolated loopback child process and embeds its real URL. Missing DSH build artifacts are reported explicitly; a fake host is never substituted.
- Code exposes editable `layout.json` plus the validated manifest, contribution JSON, generated DSH client code, and exact `cordis_define` arguments. Applying source validates it before replacing canvas state.
- KnightFrame sends a requirement with the target, full layout, selection, and adapter context into the normal conversation loop.

The designer provides target selection, component palette, direct placement, precise numeric properties, select options, disabled state, alignment, layer ordering, copy, delete, undo/redo, reset, preview, and export controls. Drag placement snaps to canvas edges/center and sibling edges/centers with live guides plus an exact coordinate badge, and dropping onto an occupied area pushes unrelated components away (minimum-vector avoidance, locked and parent/child nodes excluded). A right-click menu on any component configures its click behavior (open a page, show a notice, or none) and exposes lock/copy/order/delete shortcuts.

`Ask KnightFrame` emits one structured request to the main window containing target, selected component, current layout summary, and the user's requirement. The main window creates or selects a normal conversation and submits that request through the standard agent loop. Studio does not invoke a hidden model, store memory, or inject the whole layout into every turn.

Source editing is limited to the declarative layout source of truth. Generated adapter files remain read-only because editing generated JavaScript would bypass validation. Package publication, marketplace installation, and untrusted JavaScript execution remain unavailable until their permission and rollback contracts pass.

The DSH export contains:

- normalized KnightFrame manifest JSON;
- adapter contribution JSON for inspection;
- generated `code.client` source (`dsh-client-code.js`);
- exact `cordis_define` argument JSON using a 3-6 letter lowercase ID prefix (`cordis-define-arguments.json`);
- real Host/Client runner package identifiers and explicit approval/process-local flags (`dsh-runtime.json`).

Definition alone never runs the Package. DSH must return `pluginId` and `packageId`, after which the caller uses those exact identities with `cordis_run`.

## 7. Defaults, receipts, and token impact

- Plugin discovery is on, but only scans configured plugin roots.
- Third-party plugin activation is off until the user enables an entry.
- Plugin Studio is available locally without a model call.
- Active plugin tools are deterministically sorted and included in the stable tool prefix; inactive plugin schemas contribute zero prompt bytes.
- UI-only plugins never enter the model prompt.
- Activation, pending dependency, reload, failure, and disposal produce local receipts; raw runner logs stay in diagnostics.
- DSH cache alignment remains provider-specific: stable bytes and correct cache usage are measured, never inferred from a local hash.

## 8. Internationalization

All visible strings use `plugin.*` and `studio.*` resource keys with `en-US` and `zh-CN` parity. Plugin-supplied labels may provide a locale map; missing locales fall back to the plugin's declared default and are marked as external text.

## 9. Acceptance

Automated tests must cover:

- manifest validation and Cordis entry mapping;
- pending activation and dependency recovery;
- reverse-order effect cleanup on stop and crash;
- duplicate contribution rejection;
- JSON-RPC framing, cancellation, malformed/oversized frames, and clean EOF;
- candidate reload success and rollback;
- Rust, Node, and Python fixture runners when those runtimes exist;
- Studio add/edit/delete/undo/redo, target switch, and main-window request delivery;
- packed EXE opening Studio with no localhost, external browser, terminal flash, or orphan process;
- disabled plugins producing zero model schema bytes.

Current phase status: manifest/Cordis/framing validation, three-view Studio editing, packaged KnightFrame host preview, hidden DSH Web host launch when local build artifacts exist, native child-window bridge, structured Ask dispatch, and DSH dynamic export are implemented. Live injection into DSH's process-local dynamic runner, plugin process lifecycle, dependency recovery, transactional reload, and packed-EXE runtime acceptance remain open. Full status becomes `verified` only after every acceptance item above passes.
