;; Imported from upstream s7test.scm line 25199.
;; Original form:
;; (test (object->string (inlet 'a (let ((iter (make-iterator "123"))) (iter) iter)) :readable) "(inlet :a (make-iterator \"23\"))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (let ((iter (make-iterator "123"))) (iter) iter)) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (make-iterator \"23\"))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25199 actual expected ok?))
