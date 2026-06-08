;; Imported from upstream s7test.scm line 16419.
;; Original form:
;; (test ((lambda () (when (#_round pi) #f))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((lambda () (when (#_round pi) #f))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16419 actual expected ok?))
