# Compression format benchmark — Thu Jun 18 05:28:24 PM +03 2026

Input: medium.pkg.tar.bz3 (2383 KB)

## 1. zstd (Best for speed/ratio balance)

| Compressor | Level | Compressed | Ratio | Compress Speed | Decompress Speed |
| --- | --- | --- | --- | --- | --- |
| zstd | 1 | 2383 KB | % | 13ms | 3ms |
| zstd | 3 | 2383 KB | % | 7ms | 3ms |
| zstd | 5 | 2383 KB | % | 9ms | 4ms |
| zstd | 10 | 2383 KB | % | 17ms | 4ms |
| zstd | 15 | 2383 KB | % | 69ms | 3ms |
| zstd | 19 | 2383 KB | % | 280ms | 3ms |
| zstd | 22 | 2383 KB | % | 277ms | 4ms |
## 2. xz/lzma (Best compression ratio)

| Compressor | Level | Compressed | Ratio | Compress Speed | Decompress Speed |
| --- | --- | --- | --- | --- | --- |
| xz | 1 | 2383 KB | % | 533ms | 5ms |
| xz | 3 | 2383 KB | % | 486ms | 4ms |
| xz | 6 | 2383 KB | % | 457ms | 4ms |
| xz | 9 | 2383 KB | % | 465ms | 4ms |
## 3. lz4 (Fastest decompression)

| Compressor | Level | Compressed | Ratio | Compress Speed | Decompress Speed |
| --- | --- | --- | --- | --- | --- |
| lz4 | 1 | 2383 KB | % | 5ms | 6ms |
| lz4 | 6 | 2383 KB | % | 47ms | 5ms |
| lz4 | 9 | 2383 KB | % | 46ms | 5ms |
| lz4 | 12 | 2383 KB | % | 58ms | 5ms |
## 4. brotli (Best ratio for web/text)

| Compressor | Level | Compressed | Ratio | Compress Speed | Decompress Speed |
| --- | --- | --- | --- | --- | --- |
| brotli | 1 | 2383 KB | % | 12ms | 6ms |
| brotli | 4 | 2383 KB | % | 9ms | 5ms |
| brotli | 6 | 2383 KB | % | 12ms | 4ms |
| brotli | 9 | 2383 KB | % | 39ms | 4ms |
