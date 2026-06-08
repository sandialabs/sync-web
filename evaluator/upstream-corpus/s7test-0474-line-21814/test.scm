;; Imported from upstream s7test.scm line 21814.
;; Original form:
;; (test (format #f "~a~A~a" 1 2 3) "123")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~a~A~a" 1 2 3))))
       (expected (upstream-safe (lambda () "123")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21814 actual expected ok?))
