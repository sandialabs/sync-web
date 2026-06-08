(list
  (catch #t (lambda () (+ 1 'a)) (lambda args args))
  (catch #t (lambda () (car '())) (lambda args args))
  (catch #t (lambda () ((lambda (x) x))) (lambda args args))
  (catch #t (lambda () (let ((x 1)) y)) (lambda args args)))
