;; Imported from upstream s7test.scm line 35640.
;; Original form:
;; (test (let () (define (f) (and (values #f 1 2) 1 (subvector (vector 0) 0 0))) (f) (f)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (f) (and (values #f 1 2) 1 (subvector (vector 0) 0 0))) (f) (f)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35640 actual expected ok?))
