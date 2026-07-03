(define (make-shape n)
  (hash-table 'id n
              'name (string-append "n" (number->string n))
              'vec (vector n (+ n 1) (list n (+ n 2)))))

(let ((a (make-vector 128)) (b (make-vector 128)))
  (let init ((i 0))
    (if (= i 128)
        #t
        (begin (set! (a i) (make-shape i)) (set! (b i) (make-shape i)) (init (+ i 1)))))
  (let loop ((i 0) (acc 0))
    (if (= i 25000)
        acc
        (loop (+ i 1)
              (+ acc (if (equal? (a (remainder i 128)) (b (remainder i 128))) 1 0))))))
