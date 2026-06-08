(define (make-adder x)
  (lambda (y) (+ x y)))
(let ((f1 (make-adder 1))
      (f2 (make-adder 2))
      (f3 (make-adder 3)))
  (let loop ((i 0) (acc 0))
    (if (= i 200000)
        acc
        (loop (+ i 1) (+ acc (f1 i) (f2 i) (f3 i))))))
