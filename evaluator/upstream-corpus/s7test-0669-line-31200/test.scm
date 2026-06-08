;; Imported from upstream s7test.scm line 31200.
;; Original form:
;; (test (let ((oar #f)) (set! oar or) (oar #f #f 123)) 123)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((oar #f)) (set! oar or) (oar #f #f 123)))))
       (expected (upstream-safe (lambda () 123)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31200 actual expected ok?))
