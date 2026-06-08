;; Imported from upstream s7test.scm line 16445.
;; Original form:
;; (test (catch #t
;;         (lambda ()
;;           (let-temporarily (((*s7* 'print-length) 123123))
;;             (+ 1 #())
;;             323))
;;         (lambda (type info) 'catch1))
;;       'catch1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t
        (lambda ()
          (let-temporarily (((*s7* 'print-length) 123123))
            (+ 1 #())
            323))
        (lambda (type info) 'catch1)))))
       (expected (upstream-safe (lambda () 'catch1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16445 actual expected ok?))
