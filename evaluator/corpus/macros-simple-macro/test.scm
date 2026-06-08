(define-macro (when test . body)
  `(if ,test (begin ,@body)))

(let ((x 0))
  (when #t (set! x 12))
  (when #f (set! x 99))
  (list 'x x))
