(define-macro (twice expr)
  `(+ ,expr ,expr))

(let ((m twice))
  (list
    (list 'macro? (macro? m))
    (list 'call (m 21))))
