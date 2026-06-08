;; Imported from upstream s7test.scm line 31258.
;; Original form:
;; (test (let () (and (define (hi a) a)) (hi 1)) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (and (define (hi a) a)) (hi 1)))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31258 actual expected ok?))
