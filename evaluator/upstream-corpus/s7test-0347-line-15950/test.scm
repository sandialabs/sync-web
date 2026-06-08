;; Imported from upstream s7test.scm line 15950.
;; Original form:
;; (test (let ((L (list 0))) (set-cdr! L L) (format #f "(~S~{~^ ~S~})~%" '+ L)) "(+ 0)\n")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((L (list 0))) (set-cdr! L L) (format #f "(~S~{~^ ~S~})~%" '+ L)))))
       (expected (upstream-safe (lambda () "(+ 0)\n")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15950 actual expected ok?))
