;; Imported from upstream s7test.scm line 35618.
;; Original form:
;; (test (let ((x #(32 33))) ((values x) 0)) 32)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x #(32 33))) ((values x) 0)))))
       (expected (upstream-safe (lambda () 32)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35618 actual expected ok?))
