;; Imported from upstream s7test.scm line 35686.
;; Original form:
;; (test (+ (eval-string "(values 1 2 3)")) 6)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (eval-string "(values 1 2 3)")))))
       (expected (upstream-safe (lambda () 6)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35686 actual expected ok?))
