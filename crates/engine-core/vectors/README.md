# Golden vectors

These JSON files are copied verbatim from the Ruby engine's repository,
[calebl/text-adventure](https://github.com/calebl/text-adventure), directory
`test/engine_vectors/`, at commit `49e5388ea768dbe75814157177f4a51912cc56a1`.
Each holds one portion of the engine's pure rules as cases of named inputs and
the exact output the Ruby code gives for them. `docs/engine-vectors.md` in that
repository documents the format (version 1).

Never edit them by hand. To refresh them, in the Ruby engine's repository:

```sh
bin/rails engine:vectors
```

then copy `test/engine_vectors/*.json` here, update the commit above, and run
`cargo test -p engine-core`. A case that fails is a rule the Ruby engine
changed; port the change. A new format version fails every portion until the
harness in `tests/vectors/main.rs` learns to read it.
