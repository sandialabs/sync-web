;; Imported from upstream s7test.scm line 35634.
;; Original form:
;; (test ((object->string values) (abs 1)) #\a)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((object->string values) (abs 1)))))
       (expected (upstream-safe (lambda () #\a)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35634 actual expected ok?))
