;; Imported from upstream s7test.scm line 15116.
;; Original form:
;; (test (let ((lst (list 1 2 3))) (set! (cdr (cdr (cdr lst))) (cdr (cdr lst))) (object->string lst)) "(1 2 . #1=(3 . #1#))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1 2 3))) (set! (cdr (cdr (cdr lst))) (cdr (cdr lst))) (object->string lst)))))
       (expected (upstream-safe (lambda () "(1 2 . #1=(3 . #1#))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15116 actual expected ok?))
