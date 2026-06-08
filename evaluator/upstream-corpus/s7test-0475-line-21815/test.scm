;; Imported from upstream s7test.scm line 21815.
;; Original form:
;; (test (format #f "~a~~~a" 1 3) "1~3")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~a~~~a" 1 3))))
       (expected (upstream-safe (lambda () "1~3")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21815 actual expected ok?))
