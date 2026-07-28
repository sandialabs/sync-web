(lambda (assertions-src standard-src)

  (eval assertions-src)

  (define standard-class ((eval standard-src) 'class))
  (define standard-module (eval standard-src))
  (define standard-portable
    (standard-module 'local (standard-module 'class) (standard-module 'make)))
  (define* (standard (operation #f))
    (case operation
      ((make) (lambda (class) (standard-module 'make class)))
      ((init) (lambda (class . args) (standard-module 'init class args)))
      ((local) (lambda (class node) (standard-module 'local class node)))
      (else (if operation (standard-portable operation) (standard-portable)))))

  (assert (standard) (lambda (x) (sync-node? x)))

  (define foo-cls
    '(define-class (foo)
       (define-method (add self x) (+ x 1))
       (define-method (fact self x (result 1))
         (if (< x 1) result
             ((self 'fact) (- x 1) (* result x))))
       (define-method (state-get self)
         (byte-vector->expression (sync-cdr (self))))
       (define-method (state-set! self x)
         (set! (self) (sync-cons (sync-car (self))
                                 (expression->byte-vector x))))
       (define-method (fail-after-set! self x)
         ((self 'state-set!) x)
         (error 'expected-error "Failure after mutation"))))

  (define foo-obj (assert ((standard 'make) foo-cls) sync-node?))
  (define foo (sync-eval foo-obj))

  (assert ((foo 'add) 1) 2)

  (assert ((foo 'fact) 4) (* 4 3 2 1))

  (assert ((foo 'state-set!) "hello, world!") #t)

  (assert ((foo 'state-get)) "hello, world!")

  ;; Trusted local wrappers preserve the portable object/state protocol while
  ;; dispatching ordinary Scheme values directly.
  (let* ((node ((standard 'make) foo-cls))
         (portable (sync-eval node))
         (local ((standard 'local) foo-cls node)))
    (assert (local) sync-node?)
    (assert (local '*api*) (portable '*api*))
    (assert ((local 'add) 1) ((portable 'add) 1))
    (assert ((local 'fact) 4) ((portable 'fact) 4))
    (assert ((local 'state-set!) '(ordinary local value)) #t)
    (assert ((portable 'state-set!) '(ordinary local value)) #t)
    (assert (sync-digest (local)) (sync-digest (portable)))
    (assert (catch #t
                   (lambda () ((local 'fail-after-set!) 'changed) #f)
                   (lambda args #t))
            #t)
    (assert ((local 'state-get)) '(ordinary local value))
    (assert (catch #t
                   (lambda () ((portable 'fail-after-set!) 'changed) #f)
                   (lambda args #t))
            #t)
    (assert ((portable 'state-get)) '(ordinary local value))
    (assert ((local 'state-set!) '(second local value)) #t)
    (assert ((local 'state-get)) '(second local value)))

  (let* ((class
          '(define-class (local-only)
             (define-method (apply-procedure self proc value) (proc value))
             (define-method (return-procedure self value)
               (lambda (other) (+ value other)))))
         (node ((standard 'make) class))
         (local ((standard 'local) class node))
         (portable (sync-eval node)))
    (assert ((local 'apply-procedure) (lambda (x) (+ x 2)) 3) 5)
    (assert (((local 'return-procedure) 4) 5) 9)
    ;; Direct loading exposes raw mechanics for trusted callers; it does not
    ;; authorize host code to execute shared objects outside sync-let.
    (assert ((portable 'apply-procedure) (lambda (x) x) 1) 1)
    (assert (catch #t
                   (lambda ()
                     (sync-let ((node node) (proc (lambda (x) x)))
                       (((sync-eval node) 'apply-procedure) proc 1))
                     #f)
                   (lambda args #t))
            #t))

  (let* ((local-standard standard)
         (query '(lambda (node)
                   (let ((object (sync-eval node)))
                     ((object 'state-set!) 5)
                     ((object 'state-get)))))
         (serialized ((local-standard 'serialize) foo-obj query))
         (deserialized ((local-standard 'deserialize) serialized))
         (reordered ((local-standard 'deserialize) (reverse serialized)))
         (secret-value 'interface-lexical-capability)
         (custom
          (sync-cons
           (expression->byte-vector
            '(lambda (state)
               (lambda* (arg)
                 (cond ((not arg) state)
                       ((eq? arg 'get) (lambda (key) secret-value))
                       (else (error 'method-error "Method not recognized: ~S" arg))))))
           (sync-null)))
         (custom-copy
          ((local-standard 'deserialize)
           ((local-standard 'serialize) custom #f))))
    (assert (catch #t
                   (lambda ()
                     ((local-standard 'serialize) foo-obj (lambda (node) node))
                     #f)
                   (lambda args #t))
            #t)
    ;; Child/custom object code runs inside the shared computation boundary
    ;; and cannot resolve lexical capabilities from trusted Interface code.
    (assert (catch #t
                   (lambda () ((local-standard 'deep-get) custom '(value)) #f)
                   (lambda args #t))
            #t)
    (assert (catch #t
                   (lambda () ((local-standard 'deep-get) custom-copy '(value)) #f)
                   (lambda args #t))
            #t)
    (assert ((eval query) deserialized) 5)
    ;; JSON objects do not preserve the serializer's dependency order.
    (assert ((eval query) reordered) 5))

  (assert (catch #t
                 (lambda ()
                   ((standard 'deserialize)
                    '((n-1 (c (begin (error 'executed "untrusted code ran"))))))
                   'no-error)
                 (lambda args (car args)))
          'serialization-error)
  (assert (catch #t
                 (lambda () ((standard 'deserialize) '((n-1 (missing n-0)))) 'no-error)
                 (lambda args (car args)))
          'serialization-error)
  (assert (catch #t
                 (lambda ()
                   ((standard 'deserialize) '((n-1 (p n-2 n-0)) (n-2 (p n-1 n-0))))
                   'no-error)
                 (lambda args (car args)))
          'serialization-error)

  (define class-1-cls
    '(define-class (class-1)
       (define-method (get self ~index)
         (self '(1)))
       (define-method (set! self ~index value)
         (set! (self '(1)) value))
       (define-method (slice! self ~index) #t)
       (define-method (prune! self ~index) #t)))

  (define class-2-cls
    '(define-class (class-2)
       (define-method (*init* self)
         (set! (self '(1)) (sync-cons (sync-null) (sync-null))))
       (define-method (get self index)
         (self `(1 ,index)))
       (define-method (set! self index value)
         (set! (self `(1 ,index)) value))
       (define-method (slice! self index)
         (let ((other (if (= index 0) 1 0)))
           (set! (self `(1 ,other)) (sync-cut (self `(1 ,other))))))
       (define-method (prune! self index)
         (set! (self `(1 ,index)) (sync-cut (self `(1 ,index)))))))

  (define class-3-cls
    '(define-class (class-3)
       (define-method (*init* self)
         (set! (self '(1)) (expression->byte-vector 0)))
       (define-method (get self ~key)
         (byte-vector->expression (self '(1))))
       (define-method (set! self ~key value)
         (set! (self '(1)) (expression->byte-vector value)))
       (define-method (increment! self)
         (let ((previous (byte-vector->expression (self '(1)))))
           (set! (self '(1)) (expression->byte-vector (+ previous 1)))))))

  (define object-1 (assert ((standard 'make) class-1-cls) sync-node?))

  (define object-2 (assert ((standard 'init) class-2-cls) sync-node?))

  (define object-3 (assert ((standard 'init) class-3-cls) sync-node?))

  (let ((object (sync-eval object-2)))
    (assert ((object 'set!) 1 object-3) #t)
    (set! object-2 (object)))

  (let ((object (sync-eval object-1)))
    (assert ((object 'set!) "some path" object-2) #t)
    (set! object-1 (object)))

  (assert ((standard 'deep-get) object-1 '("some path" 1 #f)) 0)

  (let* ((object object-1)
         (object ((standard 'deep-set!) object '("some path" 1 #f) 4)))
    (assert (sync-node? object) #t)
    (assert ((standard 'deep-get) object '("some path" 1 #f)) 4))

  (let* ((object object-1)
         (object ((standard 'deep-slice!) object '("some path" 1))))
    (assert (sync-node? object) #t)
    (assert ((standard 'deep-get) object '("some path" 1 #f)) 0))

  (let* ((object object-1)
         (object ((standard 'deep-prune!) object '("some path" 1))))
    (assert (sync-node? object) #t)
    (assert ((standard 'deep-get) object '("some path" 1)) '(unknown)))

  (let* ((object object-1)
         (object ((standard 'deep-copy!) object '("some path" 1) '("some path" 0))))
    (assert (sync-node? object) #t)
    (assert ((standard 'deep-get) object '("some path" 0 #f)) 0))

  (let* ((object object-1)
         (object ((standard 'deep-merge!) object object-1)))
    (assert (sync-node? object) #t)
    (assert object object-1)
    (assert ((standard 'deep-get) object '("some path" 1 #f)) 0))

  (let* ((object object-1)
         (object ((standard 'deep-call!) object '("some path" 1) '(lambda (obj) ((obj 'increment!))))))
    (assert (sync-node? object) #t)
    (let* ((object ((standard 'deep-call!) object '("some path" 1) '(lambda (obj) ((obj 'increment!))))))
      (assert (sync-node? object) #t)
      (assert ((standard 'deep-call) object '("some path" 1) '(lambda (obj) ((obj 'get) #f))) 2)))

  (append "Success (" (object->string asserted) " checks)"))
