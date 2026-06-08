;; Imported from upstream s7test.scm line 16704.
;; Original form:
;; (test (catch #t (lambda () (make-hash-table 8 eq? (cons integer? ()))) (lambda (type info) (apply format #f info)))
;;         "make-hash-table third argument, (integer?), is a pair but should be (key-type . value-type)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (make-hash-table 8 eq? (cons integer? ()))) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "make-hash-table third argument, (integer?), is a pair but should be (key-type . value-type)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16704 actual expected ok?))
