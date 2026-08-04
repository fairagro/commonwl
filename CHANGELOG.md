# Changelog

## 0.11.0
### Added
- Report timings by Engine

## 0.10.0
### Added
- Ergonomic functions for all `CWLDocument` types

## 0.9.0

### Added
- CWL documents are now validated against the spec when loaded. Documents with duplicate
  `id`s among inputs/outputs/steps, duplicate requirement classes in one `requirements` list,
  or missing required fields on workflow step inputs/outputs are now rejected with a clear
  error at load time instead of being accepted and potentially causing a crash later during
  execution.
- `.dev/tes_env.sh` created that operates on a local Funnel TES server with rustfs S3 compatible storage

### Fixed
- Local backend: a document with `scatter: []` (scattering over an empty list) no longer
  panics.
- Local backend: fixed a case where copying inputs/outputs could silently produce an
  empty (0-byte) file instead of the real content.
- `InitialWorkDirRequirement` listings now reject more `..`-based path traversal attempts in
  `entryname` (previously only a leading `../` was caught; embedded `..` segments now are too).
- Local backend: output files are no longer left as broken/dangling references if the run's
  working directory and the requested output directory live on different filesystems or
  drives.
- Documents that reference themselves in a cycle are now rejected with a clear error instead
  of crashing: a workflow whose step (directly or transitively) runs itself, a packed CWL
  file where `$import`s form a loop, or a `Directory` input containing a symlink loop.
- A `DockerRequirement` specifying only `dockerFile` (no `dockerImageId`) no longer crashes
  the local backend; an image tag is now generated automatically.
- The TES backend now reports a clear error for a `DockerRequirement` that needs building
  from a `dockerFile`, instead of silently ignoring it and using the wrong container image.
- TES backend conformance raised from 24% to 99%, now matching Docker's own
  conformance ceiling exactly, via a series of fixes:
  - `secondaryFiles` handling is now storage-aware instead of assuming a local file path.
  - File metadata (size/checksum/contents) is now computed correctly once a value's location
    becomes remote, e.g. after crossing a workflow step boundary.
  - The TES backend no longer wires the GA4GH `stdin` executor field, which some TES servers
    (e.g. Funnel) truncate to zero bytes as soon as the task starts; the resolved stdin path
    remains reachable via the existing trailing positional command argument.
  - `runtime.outdir`/`runtime.tmpdir` no longer crash after a task runs on remote storage.
  - Empty directories - both ones staged via `InitialWorkDirRequirement` and ones a tool
    creates itself (e.g. `mkdir -p`) - are now represented correctly on S3, which has no
    native concept of an empty directory.
  - Fixed every `InitialWorkDirRequirement`-staged file/directory upload silently landing at
    the S3 bucket root instead of the run's own prefix, which could also cause an unrelated
    object elsewhere in the bucket to be swept up as if it belonged to the current run.
  - Fixed `cwl.output.json`-declared outputs (`path`/`location`) resolving to the same wrong,
    unprefixed S3 location.
  - Directory-typed `outputBinding: {glob: ...}` now matches a directory-shaped S3 prefix;
    previously it only ever matched individual object keys.
  - Fixed a regression where a tool relying on its Docker image's own `ENTRYPOINT` (rather
    than a bare shell command) stopped working under TES.
  - Output values produced via `outputEval` (including nested `File`/`Directory` values
    inside a `record`) now resolve their source location correctly instead of failing when a
    bare `runtime.outdir`-relative path needed rebasing onto the run's remote storage prefix.
  - `secondaryFiles` existence checks now recognize a directory-shaped secondary file, not
    just a file.
  - Downloading a directory whose objects have a leading slash in their key-relative path no
    longer fails outright.
  - `outputBinding: {glob: ...}` patterns built from `$(runtime.outdir)` now match correctly
    instead of never matching anything.
  - `InitialWorkDirRequirement` entries with an absolute, out-of-convention `entryname` (e.g.
    mapped to a path inside the container that isn't under the working directory) are now
    mounted correctly when `DockerRequirement` is a real `requirements` entry, and are still
    correctly rejected when it's only a `hints` entry.
  - Directory-typed `outputBinding: {glob: ...}` results are now returned in a deterministic,
    sorted order instead of an arbitrary hash-based one.
  - `glob: .` and `outputBinding: {glob: $(runtime.outdir)}` (matching the output directory
    itself) now work instead of always returning nothing.
  - A remote directory glob match now gets a real listing built from storage instead of
    always being empty, so `loadListing` + `outputEval` on a directory works the same as on
    Local/Docker.
  - `$(runtime.tmpdir)` is now a real writable directory in the task container, matching
    Docker's behavior; previously any write to it failed outright.
  - Fixed a latent bug that could corrupt S3 listing/prefix-matching for any directory-shaped
    path whose URL already carried a trailing slash.
  - A workflow step's `Directory` input with `loadListing` set no longer surfaces the internal
    `.cwl_empty_dir` marker as if it were a real file.
  - A filename containing `:` (e.g. `stdout: re:sult`) no longer breaks task submission or
    loses everything before the colon.
  - `outputBinding: {glob: dir/*}` no longer also matches files nested deeper than one level
    (e.g. `dir/sub/file`).
- Fixed several cases where a tool's output handling could crash instead of failing cleanly:
  combining `secondaryFiles` with an output constructed via `outputEval`, a workflow step
  input using `loadContents` on a `default` file value, and a tool's `cwl.output.json`
  containing an incomplete file/directory entry.

### Performance
- Local backend now hardlinks input/output files instead of always copying them when the
  source and destination are on the same filesystem, meaningfully reducing I/O for large
  files and directories.
- JavaScript expression libraries (`InlineJavascriptRequirement.expressionLib` `$include`
  files) are now cached instead of being re-read from disk on every single expression
  evaluation. The cache is invalidated automatically if the library file changes on disk.

## Start of Changelog: v0.8.5
Init Changelog