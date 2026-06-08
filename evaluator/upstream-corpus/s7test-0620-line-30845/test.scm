;; Imported from upstream s7test.scm line 30845.
;; Original form:
;; (test (let ((hi 0)) (set! hi 32)) 32)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((hi 0)) (set! hi 32)))))
       (expected (upstream-safe (lambda () 32)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30845 actual expected ok?))
