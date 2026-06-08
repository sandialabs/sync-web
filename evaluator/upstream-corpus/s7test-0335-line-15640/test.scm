;; Imported from upstream s7test.scm line 15640.
;; Original form:
;; (test (let ((ctr 0)) (for-each (lambda (a) (set! ctr (+ ctr a))) '(1 2 . 3)) ctr) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((ctr 0)) (for-each (lambda (a) (set! ctr (+ ctr a))) '(1 2 . 3)) ctr))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15640 actual expected ok?))
