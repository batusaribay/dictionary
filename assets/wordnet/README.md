# Bundled WordNet data

These files are gzip-compressed copies of the WordNet 3.0 database distributed
by Princeton University:

- Source: <https://wordnet.princeton.edu/>
- Archive: <https://wordnetcode.princeton.edu/3.0/WordNet-3.0.tar.gz>
- Files used: `dict/data.{noun,verb,adj,adv}`, `dict/index.{noun,verb,adj,adv}`,
  `dict/{noun,verb,adj}.exc`

They are embedded into the executable with `include_bytes!` and decompressed
lazily in memory. See [`LICENSE`](LICENSE) for the WordNet license.

WordNet 3.0 Copyright 2006 by Princeton University. All rights reserved.
