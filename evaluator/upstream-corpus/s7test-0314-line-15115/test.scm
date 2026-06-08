;; Imported from upstream s7test.scm line 15115.
;; Original form:
;; (test (let ((lst (list 1 2 3))) (set! (cdr (cdr (cdr lst))) (cdr lst)) (object->string lst)) "(1 . #1=(2 3 . #1#))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1 2 3))) (set! (cdr (cdr (cdr lst))) (cdr lst)) (object->string lst)))))
       (expected (upstream-safe (lambda () "(1 . #1=(2 3 . #1#))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15115 actual expected ok?))
