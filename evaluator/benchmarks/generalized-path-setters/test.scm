(let ((root (hash-table 'env (inlet 'items (make-vector 32)) 'bytes (make-byte-vector 64 0))))
  (let init ((i 0))
    (if (= i 32)
        #t
        (begin
          (set! (((root 'env) 'items) i) (vector (list i 0) (hash-table 'seen 0)))
          (init (+ i 1)))))
  (let loop ((i 0) (acc 0))
    (if (= i 40000)
        acc
        (let* ((idx (remainder i 32))
               (item (((root 'env) 'items) idx))
               (pair ((item 0) 0))
               (stats (item 1)))
          (set! ((item 0) 1) (+ ((item 0) 1) 1))
          (set! (stats 'seen) (+ (stats 'seen) 1))
          (set! ((root 'bytes) (remainder i 64)) (remainder (+ i idx) 256))
          (loop (+ i 1) (+ acc pair ((item 0) 1) (stats 'seen)))))))
