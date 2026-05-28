cwlVersion: v1.2
class: CommandLineTool
baseCommand: [python3, load.py]
requirements:
  DockerRequirement:
    dockerPull: python:3.10-slim
  InitialWorkDirRequirement:
    listing:
      - class: Dirent
        entry:
          $include: load.py
        entryname: load.py
inputs: []
outputs: []