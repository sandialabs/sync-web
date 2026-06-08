;; Imported from upstream s7test.scm line 31199.
;; Original form:
;; (test (let ((oar or)) (oar #f 43)) 43)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((oar or)) (oar #f 43)))))
       (expected (upstream-safe (lambda () 43)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31199 actual expected ok?))
