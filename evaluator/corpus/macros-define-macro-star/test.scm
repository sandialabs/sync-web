(define-macro* (incf place (amount 1))
  `(set! ,place (+ ,place ,amount)))

(let ((x 10))
  (incf x)
  (incf x 5)
  (incf x :amount 7)
  x)
