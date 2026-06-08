;; Imported from upstream s7test.scm line 35696.
;; Original form:
;; (test (let ((x 1)) (set! x (values)) x) #<unspecified>)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x 1)) (set! x (values)) x))))
       (expected (upstream-safe (lambda () #<unspecified>)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35696 actual expected ok?))
