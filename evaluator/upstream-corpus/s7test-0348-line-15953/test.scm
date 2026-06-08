;; Imported from upstream s7test.scm line 15953.
;; Original form:
;; (test (length (let ((E (inlet 'value 0))) (varlet E 'self E))) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (length (let ((E (inlet 'value 0))) (varlet E 'self E))))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15953 actual expected ok?))
