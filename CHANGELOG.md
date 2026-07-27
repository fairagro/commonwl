# Changelog

## [Unreleased]

### Added
- CWL documents are now validated against the spec when loaded. Documents with duplicate
  `id`s among inputs/outputs/steps, duplicate requirement classes in one `requirements` list,
  or missing required fields on workflow step inputs/outputs are now rejected with a clear
  error at load time instead of being accepted and potentially causing a crash later during
  execution.

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

### Performance
- Local backend now hardlinks input/output files instead of always copying them when the
  source and destination are on the same filesystem, meaningfully reducing I/O for large
  files and directories.
- JavaScript expression libraries (`InlineJavascriptRequirement.expressionLib` `$include`
  files) are now cached instead of being re-read from disk on every single expression
  evaluation. The cache is invalidated automatically if the library file changes on disk.
