;; Imported from upstream s7test.scm line 25111.
;; Original form:
;; (test (object->string (let ((iter (make-iterator (hash-table)))) (iter) iter) :readable) "(make-iterator (hash-table))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (let ((iter (make-iterator (hash-table)))) (iter) iter) :readable))))
       (expected (upstream-safe (lambda () "(make-iterator (hash-table))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25111 actual expected ok?))
