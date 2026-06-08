;; Imported from upstream s7test.scm line 21849.
;; Original form:
;; (test (format #f "[~NC]" 1 #\a) "[a]")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "[~NC]" 1 #\a))))
       (expected (upstream-safe (lambda () "[a]")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21849 actual expected ok?))
