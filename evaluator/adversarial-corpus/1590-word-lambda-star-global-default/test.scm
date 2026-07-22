(define d 3)
(define* (f (x d)) x)
(let ((a (let loop ((i 0))
           (if (= i 1) (f) (loop (+ i 1))))))
  (set! d 4)
  (list a
        (let loop ((i 0))
          (if (= i 1) (f) (loop (+ i 1))))))
