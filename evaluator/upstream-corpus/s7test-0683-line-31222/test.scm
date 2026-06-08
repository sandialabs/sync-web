;; Imported from upstream s7test.scm line 31222.
;; Original form:
;; (test (and (if #f #f)) (if #f #f))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and (if #f #f)))))
       (expected (upstream-safe (lambda () (if #f #f))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31222 actual expected ok?))
