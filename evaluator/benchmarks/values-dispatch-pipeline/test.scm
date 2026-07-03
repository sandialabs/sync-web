(define (split n) (values (quotient n 3) (remainder n 3)))
(define (combine a b) (+ (* a 7) (* b 11)))
(define (dispatch op . args)
  (case op
    ((combine) (apply combine args))
    (else 0)))

(let loop ((i 0) (acc 0))
  (if (= i 70000)
      acc
      (loop (+ i 1) (+ acc (dispatch 'combine (split i))))))
