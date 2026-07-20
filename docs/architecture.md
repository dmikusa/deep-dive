# Architecture (C4 Model)

## C1 — System Context

```mermaid
C4Context
  title C1 — System Context: deep-dive

  Person(endUser, "End User", "Developer or operator exploring Docker/OCI image layers")

  System_Boundary(dd, "deep-dive") {
    System(sys, "deep-dive", "Interactive CLI tool for exploring Docker/OCI image layer contents via a TUI")
  }

  System_Ext(docker, "Docker Daemon", "Local Docker engine (Unix socket)")
  System_Ext(registry, "OCI Registry", "Docker Hub, GHCR, etc. (HTTPS)")
  System_Ext(podman, "Podman", "Local Podman daemonless container engine")
  System_Ext(fs, "Local Filesystem", "Docker save tars, OCI layout directories, YAML config files")
  System_Ext(terminal, "Terminal", "Raw terminal I/O via crossterm")
  System_Ext(clipboard, "Clipboard", "System clipboard via arboard")

  Rel(endUser, sys, "Runs deep-dive with image URI")
  Rel(sys, docker, "Inspect, pull, export images via bollard (Unix socket)")
  Rel(sys, registry, "Pull manifests, configs, layer blobs via oci-client (HTTPS)")
  Rel(sys, podman, "Exec podman image save (child process)")
  Rel(sys, fs, "Read tar files, OCI layout dirs, config YAML")
  Rel(sys, terminal, "Render TUI, read keyboard input")
  Rel(sys, clipboard, "Copy layer detail field values")

  UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

## C2 — Containers

```mermaid
C4Container
  title C2 — Container Diagram: deep-dive internals

  Person(endUser, "End User", "Runs deep-dive from the terminal")

  System_Boundary(dd, "deep-dive") {
    Container(cli, "CLI", "Rust + clap", "Parses args (image URI, flags), loads config, launches TUI")
    Container(tui, "TUI", "Rust + ratatui + crossterm", "Interactive terminal UI: layer list, file tree, details, report panes")

    Container_Boundary(image, "Image Acquisition") {
      Container(resolver, "Resolver Trait", "Rust trait + impls", "Fetches images from Docker daemon, registry, Podman, or local files")
      Container(progress, "Progress System", "Rust mpsc channels", "Streams resolver progress updates to TUI loading screen")
    }

    Container_Boundary(analysis, "Analysis Pipeline") {
      Container(parser, "Archive Parser", "Rust", "Parses Docker save tars & OCI layouts into FileTrees per layer")
      Container(comparer, "Comparer", "Rust + LRU cache", "Computes diff-marked trees (Natural & Aggregated views)")
      Container(filetree, "FileTree", "Rust", "Core data structure: layered file tree with merge, diff, and render operations")
      Container(analyzers, "Analyzers", "Rust trait + 3 impls", "Efficiency, Layer Stats, and Shaded File analysis")
      Container(report, "Report", "Rust", "Collects analyzer results for TUI display")
    }

    Container(config, "Config", "Rust + serde_yaml", "Loads YAML config: keybindings, display settings, extract defaults")
    Container(utils, "Utilities", "Rust", "Formatting, sanitization, path expansion helpers")
  }

  System_Ext(docker, "Docker Daemon", "bollard (Unix socket)")
  System_Ext(registry, "OCI Registry", "oci-client (HTTPS)")
  System_Ext(podman, "Podman", "podman image save (child process)")
  System_Ext(fs, "Local Filesystem", "tar files, OCI dirs, config")
  System_Ext(terminal, "Terminal", "crossterm I/O")
  System_Ext(clipboard, "System Clipboard", "arboard")

  Rel(endUser, cli, "Invokes via shell")
  Rel(cli, config, "Reads", "deep-dive.yaml")
  Rel(cli, tui, "Launches", "image URI, analyzers, config")
  Rel(tui, resolver, "Spawns", "image URI, progress sender")
  Rel(tui, comparer, "Reads", "selected layer index")
  Rel(tui, report, "Reads", "analyzer results for display")
  Rel(resolver, progress, "Sends", "status / bytes")
  Rel(progress, tui, "Streams", "loading updates")
  Rel(resolver, parser, "Returns", "Image with raw layer data")
  Rel(parser, filetree, "Builds", "FileTree per layer")
  Rel(comparer, filetree, "Uses", "stack() + compare_and_mark()")
  Rel(analyzers, parser, "Reads", "Image with FileTrees")
  Rel(analyzers, report, "Writes", "AnalysisResult")
  Rel(resolver, docker, "Connects", "Unix socket")
  Rel(resolver, registry, "Connects", "HTTPS")
  Rel(resolver, podman, "Shells out", "podman image save")
  Rel(resolver, fs, "Reads", "local files")
  Rel(tui, terminal, "Renders / reads input")
  Rel(tui, clipboard, "Writes", "Copied values")
  Rel(tui, fs, "Extracts files", "selected file content")

  UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="2")
```

## C3 — Components (Analysis Pipeline)

```mermaid
C4Component
  title C3 — Component Diagram: Analysis Pipeline internals

  Container_Boundary(pipeline, "Analysis Pipeline") {

    Component(parse_docker, "parse_docker_save_tar", "Entry point", "Reads Docker save tar manifest.json + config, calls build_layers()")
    Component(parse_oci, "parse_oci_layout / parse_oci_archive_tar", "Entry point", "Reads OCI layout index.json + blobs, calls build_layers()")
    Component(build_layers, "build_layers", "Core builder", "Iterates layer blobs, decompresses, parses tar entries, builds FileTrees, matches history commands")
    Component(parse_entries, "parse_tar_entries", "Tar reader", "Reads tar stream: path, size, mode, type, linkname, content hash (xxHash64)")
    Component(detect_compress, "detect_compression", "Sniffer", "Detects gzip (1f 8b) or zstd (28 b5 2f fd) by magic bytes")
    Component(decompress, "decompress_to_vec", "Decompressor", "Decompresses gzip (flate2) or zstd (zstd crate) layer data")

    Component(filetree_struct, "FileTree", "Data structure", "Root FileNode with BTreeMap children; add_path, stack, compare_and_mark, render_tree")
    Component(filenode, "FileNode", "Tree node", "Path, FileInfo, children, DiffType, collapsed state")
    Component(fileinfo, "FileInfo", "File metadata", "Size, mode, uid, gid, entry type, linkname, content hash, raw content bytes")
    Component(difftype, "DiffType enum", "Change marker", "Unmodified / Added / Removed / Modified")

    Component(comparer_struct, "Comparer", "Cached engine", "Manages Vec<Layer> + LRU cache; get_tree(), natural_indexes(), aggregated_indexes(), build_cache()")
    Component(stack_range, "stack_range", "Internal", "Stacks layers in a range (whiteout-aware merge)")
    Component(build_merged, "build_merged_tree", "Internal", "Stacks lower range, stacks upper range, runs compare_and_mark()")

    Component(eff_analyzer, "EfficiencyAnalyzer", "Analyzer impl", "Tracks cumulative_size vs min_size per path. Score = min_total / cumulative_total")
    Component(layer_stats_analyzer, "LayerStatsAnalyzer", "Analyzer impl", "Per-layer metrics: unique/wasted size, file/dir/symlink/whiteout counts")
    Component(shaded_analyzer, "ShadedFileAnalyzer", "Analyzer impl", "Detects files overwritten by higher layers; enumerates all occurrences")

    Component(analyzer_trait, "Analyzer trait", "Interface", "name(), description(), analyze(&Image) -> Result<Box<dyn AnalysisResult>>")
    Component(result_trait, "AnalysisResult trait", "Interface", "analyzer_name(), summary(), details(), as_any()")
    Component(report_struct, "Report", "Collector", "Report::generate() runs all analyzers, collects Vec<Box<dyn AnalysisResult>>")
    Component(section, "AnalysisSection", "Display unit", "Title + Vec<AnalysisItem>")
    Component(item, "AnalysisItem", "Key-value pair", "Label + value strings for TUI rendering")
  }

  Rel(parse_docker, build_layers, "Calls", "layer sources + config + tags")
  Rel(parse_oci, build_layers, "Calls", "layer blob bytes + tags")
  Rel(build_layers, parse_entries, "Calls", "decompressed bytes")
  Rel(parse_entries, filetree_struct, "Calls add_path()", "path + FileInfo")
  Rel(build_layers, detect_compress, "Calls", "layer blob header")
  Rel(build_layers, decompress, "Calls", "compressed bytes → decompressed")

  Rel(filetree_struct, filenode, "Contains", "root node + children")
  Rel(filenode, fileinfo, "Contains", "per-node metadata")
  Rel(filenode, difftype, "Has", "change classification")

  Rel(comparer_struct, stack_range, "Uses", "to build trees")
  Rel(comparer_struct, build_merged, "Uses", "to produce diff-marked output")
  Rel(stack_range, filetree_struct, "Calls stack()", "merge trees")
  Rel(build_merged, filetree_struct, "Calls compare_and_mark()", "diff computation")

  Rel(eff_analyzer, analyzer_trait, "Implements")
  Rel(layer_stats_analyzer, analyzer_trait, "Implements")
  Rel(shaded_analyzer, analyzer_trait, "Implements")
  Rel(eff_analyzer, result_trait, "Produces EfficiencyResult")
  Rel(layer_stats_analyzer, result_trait, "Produces LayerStatsResult")
  Rel(shaded_analyzer, result_trait, "Produces ShadedFileResult")
  Rel(report_struct, analyzer_trait, "Calls analyze() on each")
  Rel(report_struct, result_trait, "Collects", "into Vec")
  Rel(result_trait, section, "Produces", "via details()")
  Rel(section, item, "Contains")

  UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```
