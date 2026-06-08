;; Imported from upstream s7test.scm line 35726.
;; Original form:
;; (test (catch #t
;; 	(lambda ()
;; 	  (let-temporarily (((*s7* 'print-length) (values 1 2))) 1))
;; 	(lambda (type info)
;; 	  (apply format #f info)))
;;       "let-set!: too many arguments: (let-set! *s7* print-length 1 2)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t
	(lambda ()
	  (let-temporarily (((*s7* 'print-length) (values 1 2))) 1))
	(lambda (type info)
	  (apply format #f info))))))
       (expected (upstream-safe (lambda () "let-set!: too many arguments: (let-set! *s7* print-length 1 2)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35726 actual expected ok?))
