Indicates that a workflow component should be run in a
[Docker](https://docker.com) or Docker-compatible (such as
[Singularity](https://www.sylabs.io/) and [udocker](https://github.com/indigo-dc/udocker)) container environment and
specifies how to fetch or build the image.

If a `CommandLineTool` lists `DockerRequirement` under
`hints` (or `requirements`), it may (or must) be run in the specified Docker
container.

The platform must first acquire or install the correct Docker image as
specified by `dockerPull`, `dockerImport`, `dockerLoad` or `dockerFile`.

The platform must execute the tool in the container using `docker run` with
the appropriate Docker image and tool command line.

The workflow platform may provide input files and the designated output
directory through the use of volume bind mounts. The platform should rewrite
file paths in the input object to correspond to the Docker bind mounted
locations. That is, the platform should rewrite values in the parameter context
such as `runtime.outdir`, `runtime.tmpdir` and others to be valid paths
within the container. The platform must ensure that `runtime.outdir` and
`runtime.tmpdir` are distinct directories.

When running a tool contained in Docker, the workflow platform must not
assume anything about the contents of the Docker container, such as the
presence or absence of specific software, except to assume that the
generated command line represents a valid command within the runtime
environment of the container.

A container image may specify an
[ENTRYPOINT](https://docs.docker.com/engine/reference/builder/#entrypoint)
and/or
[CMD](https://docs.docker.com/engine/reference/builder/#cmd).
Command line arguments will be appended after all elements of
ENTRYPOINT, and will override all elements specified using CMD (in
other words, CMD is only used when the `CommandLineTool` definition
produces an empty command line).

Use of implicit ENTRYPOINT or CMD are discouraged due to reproducibility
concerns of the implicit hidden execution point (For further discussion, see
[https://doi.org/10.12688/f1000research.15140.1](https://doi.org/10.12688/f1000research.15140.1)). Portable
`CommandLineTool` wrappers in which use of a container is optional must not rely on ENTRYPOINT or CMD.
`CommandLineTools` which do rely on ENTRYPOINT or CMD must list `DockerRequirement` in the
`requirements` section.

## Interaction with other requirements

If [`EnvVarRequirement`](#EnvVarRequirement) is specified alongside a
`DockerRequirement`, the environment variables must be provided to Docker
using `--env` or `--env-file` and interact with the container's preexisting
environment as defined by Docker.
