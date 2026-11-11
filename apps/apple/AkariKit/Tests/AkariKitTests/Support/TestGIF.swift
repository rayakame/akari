import Foundation

/// An animated GIF written for a test, to a byte count: every frame the whole canvas, kept
/// underneath the next (disposal 1), each after the first drawing a band of noise and leaving
/// the rest transparent, 40 ms apart (the last stays 120 ms), looping forever. The noise is sized
/// so the file comes out just under `bytes`; a comment before the trailer makes up the rest.
nonisolated enum TestGIF {
    struct Shape {
        var width: Int
        var height: Int
        var frames: Int
        var bytes: Int

        /// Every frame decoded at the file's own size, RGBA.
        var decodedBytes: Int { width * height * 4 * frames }
    }

    /// The GIF that never played in Akari: 286 MB decoded.
    static let heavy = Shape(width: 540, height: 405, frames: 327, bytes: 10_159_614)

    private static let transparent: UInt8 = 255

    static func data(_ shape: Shape) -> Data {
        let pixels = shape.width * shape.height
        var bytes = [UInt8]("GIF89a".utf8)
        bytes.reserveCapacity(shape.bytes)
        bytes += le(shape.width) + le(shape.height) + [0xf7, 0, 0]
        for index in 0...255 {
            bytes += [UInt8(index), UInt8(truncatingIfNeeded: index * 7), UInt8(truncatingIfNeeded: index * 13)]
        }
        bytes += [0x21, 0xff, 0x0b] + [UInt8]("NETSCAPE2.0".utf8) + [3, 1, 0, 0, 0]
        // Room for the comment and for a frame that came out heavier than the last.
        let target = max(0, shape.bytes - 8 * 1024)
        var seed: UInt32 = 0x2545_f491
        var cost = 1.5
        var table = [UInt16](repeating: 0, count: 4096 << 8)
        for frame in 0..<shape.frames {
            let share = max(0, target - bytes.count) / (shape.frames - frame)
            let noisy = frame == 0 ? 0 : min(pixels, Int(Double(max(0, share - 64 - pixels / 2048)) / cost))
            var canvas = [UInt8](repeating: frame == 0 ? 0 : transparent, count: pixels)
            let start = (frame * 7 * shape.width) % pixels
            for offset in 0..<noisy {
                seed = seed &* 1_664_525 &+ 1_013_904_223
                canvas[(start + offset) % pixels] = min(UInt8(seed >> 24), transparent - 1)
            }
            let before = bytes.count
            let delay: UInt8 = frame + 1 == shape.frames ? 12 : 4
            bytes += [0x21, 0xf9, 4, frame == 0 ? 0x04 : 0x05, delay, 0, transparent, 0]
            bytes += [0x2c, 0, 0, 0, 0] + le(shape.width) + le(shape.height) + [0]
            lzw(canvas, table: &table, into: &bytes)
            if noisy > 0 {
                cost = Double(bytes.count - before) / Double(noisy)
            }
        }
        precondition(bytes.count + 1 <= shape.bytes, "the frames alone weigh more than \(shape.bytes)")
        comment(shape.bytes - bytes.count - 1, into: &bytes)
        bytes.append(0x3b)
        precondition(bytes.count == shape.bytes)
        return Data(bytes)
    }

    private static func le(_ value: Int) -> [UInt8] {
        [UInt8(value & 0xff), UInt8(value >> 8)]
    }

    // A comment extension of exactly `size` bytes; nothing for 0.
    private static func comment(_ size: Int, into bytes: inout [UInt8]) {
        guard size > 0 else {
            return
        }
        precondition(size >= 5, "a comment takes at least 5 bytes")
        bytes += [0x21, 0xfe]
        // Sub-blocks of a length byte and up to 255 bytes, then the terminator.
        var left = size - 3
        while left > 0 {
            let block = left == 257 ? 255 : min(left, 256)
            bytes.append(UInt8(block - 1))
            bytes.append(contentsOf: repeatElement(UInt8(ascii: "."), count: block - 1))
            left -= block
        }
        bytes.append(0)
    }

    // `table` holds the code by prefix and pixel, 0 for none, and comes back all zero.
    private static func lzw(_ pixels: [UInt8], table: inout [UInt16], into bytes: inout [UInt8]) {
        let clear: UInt32 = 256
        let end: UInt32 = 257
        var data = [UInt8]()
        data.reserveCapacity(pixels.count / 4)
        var buffer: UInt64 = 0
        var bits: UInt64 = 0
        var width: UInt64 = 9
        func emit(_ code: UInt32) {
            buffer |= UInt64(code) << bits
            bits += width
            while bits >= 8 {
                data.append(UInt8(truncatingIfNeeded: buffer))
                buffer >>= 8
                bits -= 8
            }
        }
        var used = [Int]()
        used.reserveCapacity(4096)
        var next = end + 1
        emit(clear)
        table.withUnsafeMutableBufferPointer { table in
            pixels.withUnsafeBufferPointer { pixels in
                var prefix = UInt32(pixels[0])
                for index in 1..<pixels.count {
                    let pixel = UInt32(pixels[index])
                    let key = Int(prefix << 8 | pixel)
                    if table[key] != 0 {
                        prefix = UInt32(table[key])
                        continue
                    }
                    emit(prefix)
                    if next == 4096 {
                        emit(clear)
                        for key in used {
                            table[key] = 0
                        }
                        used.removeAll(keepingCapacity: true)
                        next = end + 1
                        width = 9
                    } else {
                        if next >= 1 << width {
                            width += 1
                        }
                        table[key] = UInt16(next)
                        used.append(key)
                        next += 1
                    }
                    prefix = pixel
                }
                emit(prefix)
                for key in used {
                    table[key] = 0
                }
            }
        }
        emit(end)
        if bits > 0 {
            data.append(UInt8(truncatingIfNeeded: buffer))
        }
        bytes.append(8)
        var offset = 0
        while offset < data.count {
            let block = min(255, data.count - offset)
            bytes.append(UInt8(block))
            bytes.append(contentsOf: data[offset..<offset + block])
            offset += block
        }
        bytes.append(0)
    }
}

extension TestGIF {
    // Written once per run, off the main actor: the encoder takes seconds in a debug build.
    private static let heavyFile = Task.detached { () throws -> URL in
        let url = FileManager.default.temporaryDirectory.appending(
            path: "akari-test-heavy-\(UUID().uuidString).gif")
        try data(heavy).write(to: url)
        atexit_b { try? FileManager.default.removeItem(at: url) }
        return url
    }

    /// `heavy` as a file, shared by the run's tests.
    static var heavyURL: URL {
        get async throws { try await heavyFile.value }
    }
}
