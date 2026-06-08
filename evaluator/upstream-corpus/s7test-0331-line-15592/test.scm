;; Imported from upstream s7test.scm line 15592.
;; Original form:
;; (test (let ((lst (list 1 2 3))) (set! (cdr (cddr lst)) lst) (object->string (append (list lst) (list lst) ()))) "(#1=(1 2 3 . #1#) #1#)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1 2 3))) (set! (cdr (cddr lst)) lst) (object->string (append (list lst) (list lst) ()))))))
       (expected (upstream-safe (lambda () "(#1=(1 2 3 . #1#) #1#)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15592 actual expected ok?))
