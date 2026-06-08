;; Imported from upstream s7test.scm line 21247.
;; Original form:
;; (test (+ 100 (with-input-from-string "123" (lambda () (values (read) 1)))) 224)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ 100 (with-input-from-string "123" (lambda () (values (read) 1)))))))
       (expected (upstream-safe (lambda () 224)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21247 actual expected ok?))
