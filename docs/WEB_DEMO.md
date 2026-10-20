# Cuboctahedron browser demo

The static site in `site/` needs no build, npm install, framework, CDN,
account or backend. Native WebGL draws the cuboctahedron; JavaScript runs
a bounded mirror of the Rust integer lattice. No third-party source or
assets are bundled.

## GitHub Pages

1. Open the repository's **Settings → Pages**.
2. Under **Build and deployment → Source**, select **GitHub Actions**.
3. Merge the demo PR into `main`. The **Deploy cuboctahedron demo** workflow
   verifies model parity, uploads only `site/`, and deploys it. If Pages was
   enabled after the merge, run that workflow manually on `main`.
4. Use the deployment URL shown by GitHub's `github-pages` environment.

A workflow cannot enable a new Pages site with its ordinary `GITHUB_TOKEN`;
that initial setting requires a repository administrator. PRs run checks
without deploying. Manual workflow runs deploy only from `main`.

## Local use

From the repository root: `python3 -m http.server 8000 --directory site`.
Open `http://localhost:8000`. ES modules require HTTP; opening `index.html`
as a `file://` URL is unsupported.

## Controls and meaning

- Drag the canvas or use arrow keys to orbit; scroll or use `+`/`-` to zoom;
  `Home` or **Reset view** restores the camera.
- Select a node or use the vertex dropdown to inspect coordinates,
  neighbours and its register value. Structure labels are vertex indices;
  lattice labels are register values 0–3.
- Highlight any of four perfect matchings, or the next scheduled matching.
  In lattice mode the highlighted triangle is the next coupling face.
- Step, play or scrub between rounds 0 and 128. All six cells evolve, even
  though the view shows one cell at a time. The partner readout describes
  the next round. Play stops at 128 and pauses when the tab is hidden.
- **Reverse to seed** runs inverse operations and reports actual restoration
  mismatches across 72 registers. It does not reset or re-seed the state.
- A new uint32 seed starts a fresh six-cell lattice.

Rendering is event-driven and uses a neutral palette with amber highlights.
Animation starts only on request. If WebGL is unavailable, the text inspector
and lattice controls continue to work. WebGL context restoration recreates
rendering resources without changing lattice state.

## Reference parity

`node tests/web.mjs` executes `cargo run --locked --example web_reference`
and compares graph coordinates, edges, triangles, matching assignments and
control faces, plus 160 complete lattice states (four seeds, two cell counts,
20 round counts through round 128). Every case is reversed to its original
registers. No golden values are manually copied from another project.

The six square faces are derived for rendering only. Browser cells are bounded
to 4096 and the UI uses six. This is a classical model demonstration, not the
Vulkan compute runner or a replacement for its hardware verification.
