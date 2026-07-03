(define (make-record id)
  (let ((e (inlet 'id id 'count 0 'child (inlet 'score (* id 2)))))
    e))

(let ((records (make-vector 64)))
  (let init ((i 0))
    (if (= i 64)
        #t
        (begin (set! (records i) (make-record i)) (init (+ i 1)))))
  (let loop ((i 0) (acc 0))
    (if (= i 50000)
        acc
        (let* ((r (records (remainder i 64)))
               (child (r 'child)))
          (set! (r 'count) (+ (r 'count) 1))
          (set! (child 'score) (+ (child 'score) (remainder i 5)))
          (loop (+ i 1) (+ acc (r 'id) (r 'count) (child 'score)))))))
