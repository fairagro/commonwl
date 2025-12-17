#!/usr/bin/env cwl-runner

cwlVersion: v1.2
class: ExpressionTool

requirements:
  InlineJavascriptRequirement: {}

inputs: []

outputs:
  output: Any
expression: "$({'output': null })"
