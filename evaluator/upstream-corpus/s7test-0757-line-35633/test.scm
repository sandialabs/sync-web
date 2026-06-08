;; Imported from upstream s7test.scm line 35633.
;; Original form:
;; (test (apply begin (values '(values "hi"))) (apply (values begin '(values "hi"))))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (apply begin (values '(values "hi"))))))
       (expected (upstream-safe (lambda () (apply (values begin '(values "hi"))))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35633 actual expected ok?))
