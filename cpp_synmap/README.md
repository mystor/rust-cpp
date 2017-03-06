# Synmap

> NOTE: This crate currently depends on `cpp_syn` rather than `syn`, as it
> requires the `Span` features which have not been landed in `syn` yet for the
> majority of its features.

This crate provides a `SourceMap` type which can be used to parse an entire
crate and generate a complete AST. It also updates the spans in the parsed AST
to be relative to the `SourceMap` rather than the bytes in the input file.

With this information, the `SourceMap` provides methods to map spans to source
filenames (`filename`), source text (`source_text`) and line/column numbers
(`locinfo`).
