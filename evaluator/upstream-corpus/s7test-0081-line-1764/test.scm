;; Imported from upstream s7test.scm line 1764.
;; Original form:
;; (test (eq? (current-input-port) (current-input-port)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (current-input-port) (current-input-port)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1764 actual expected ok?))
