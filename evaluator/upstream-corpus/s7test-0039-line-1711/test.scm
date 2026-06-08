;; Imported from upstream s7test.scm line 1711.
;; Original form:
;; (test (let ((x (vector 'a))) (eq? x x)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x (vector 'a))) (eq? x x)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1711 actual expected ok?))
