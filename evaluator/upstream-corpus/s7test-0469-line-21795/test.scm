;; Imported from upstream s7test.scm line 21795.
;; Original form:
;; (test (eq? #\tab ((format #f "\t") 0)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? #\tab ((format #f "\t") 0)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21795 actual expected ok?))
