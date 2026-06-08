;; Imported from upstream s7test.scm line 21847.
;; Original form:
;; (test (format #f "[~NC]" 0 #\a) "[]")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "[~NC]" 0 #\a))))
       (expected (upstream-safe (lambda () "[]")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21847 actual expected ok?))
