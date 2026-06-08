;; Imported from upstream s7test.scm line 16421.
;; Original form:
;; (test (let () (define (f1) (abs (#_logand))) (f1)) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (f1) (abs (#_logand))) (f1)))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16421 actual expected ok?))
