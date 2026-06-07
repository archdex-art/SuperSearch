;; SuperSearch WASM extension (sample) — conforms to the host ABI:
;;   exports: memory, alloc(i32)->i32, query(i32,i32)->i64
;; query ignores its input and returns a packed (out_ptr<<32 | out_len)
;; pointing at a static JSON result array in linear memory.
(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 1024))
  ;; 29-byte JSON at offset 16: [{"title":"hello from wasm"}]
  (data (i32.const 16) "[{\"title\":\"hello from wasm\"}]")

  ;; Bump allocator: hand out `n` bytes from the heap and advance it.
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))

  ;; Return the static result regardless of input.
  (func (export "query") (param i32 i32) (result i64)
    (i64.or
      (i64.shl (i64.const 16) (i64.const 32))
      (i64.const 29))))
