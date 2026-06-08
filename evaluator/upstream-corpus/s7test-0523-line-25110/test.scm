;; Imported from upstream s7test.scm line 25110.
;; Original form:
;; (test (object->string (let ((iter (make-iterator #(1)))) (iter) (iter) iter) :readable) "(make-iterator #())")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (let ((iter (make-iterator #(1)))) (iter) (iter) iter) :readable))))
       (expected (upstream-safe (lambda () "(make-iterator #())")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25110 actual expected ok?))
