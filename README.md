# Knot Downloader

Small companion for [Knot Resolver](https://www.knot-resolver.cz/) that keeps Response Policy Zone (RPZ) files up to date. The tool polls one or more remote RPZ sources, writes them locally, and skips downloads when the content is unchanged (via `ETag`).

## Features
- Polls multiple RPZ endpoints and writes them to disk on a fixed interval
- Respects `ETag` headers to avoid unnecessary downloads
- Ships as a static binary; runnable directly or via Docker

## Usage (CLI)
- Create a config file (default `config.yml` or override with `CONFIG_PATH`) matching:
  ```yaml
  interval: 1h
  create_directories: true
  files:
    - url: https://o0.pages.dev/Lite/rpz.txt
      path: data/o0-lite.rpz
    - url: https://big.oisd.nl/rpz
      path: data/oisd-big.rpz
  ```
  `interval` accepts human-readable durations; set `create_directories` to `false` if you want to manage folders yourself.
- Run from source:
  ```bash
  cargo run --release

## Usage (Docker)
- Build the image locally (repository name `toogle/knot-downloader` is assumed):
  ```bash
  docker build -t toogle/knot-downloader .
  ```
- Run the container, mounting your config and output directory:
  ```bash
  docker run --rm \
    -v $(pwd)/config.yml:/config.yml:ro \
    -v $(pwd)/data:/data \
    -e CONFIG_PATH=/config.yml \
    ghcr.io/toogle/knot-downloader
  ```
  Adjust the mounted paths to match your environment; `CONFIG_PATH` can point anywhere inside the container.

## License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
