;; Imported from upstream s7test.scm line 35600.
;; Original form:
;; (test (string-ref (values "hiho" 2)) #\h)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (string-ref (values "hiho" 2)))))
       (expected (upstream-safe (lambda () #\h)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35600 actual expected ok?))
