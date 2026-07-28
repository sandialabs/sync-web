(let ((class '
(define-class (standard)
  ;; Standard class builds and manipulates sync objects generically.

  (define-method (make self class)
    ;; Build an uninitialized object shell from a define-class form.
    (if (not (eq? (car class) 'define-class))
        (error 'class-error "Expected define-class form, got: ~S" class))

    (let* ((name (caadr class))
           (methods (let loop ((body (cddr class)) (methods '()))
                      (cond ((null? body) (reverse methods))
                            ((string? (car body)) (loop (cdr body) methods))
                            (else (if (not (eq? (caar body) 'define-method))
                                      (error 'class-error
                                             "Expected define-method in class ~S, got: ~S" name (car body)))
                                  (loop (cdr body)
                                        (cons `(,(caadar body) (lambda* ,(cdadar body) ,@(cddar body))) methods))))))
           (api (let ((proc (lambda (x) (append " " (symbol->string (car x))))))
                  (substring (apply append (map proc methods)) 1)))
           (description (append "--- Standard Class ---\n"
                                "Name: " (symbol->string name) "\n"
                                "Description: " (if (string? (caddr class)) (caddr class) "") "\n"
                                "Functions: " api "\n"
                                "-------------------------"))
           (err '(error 'method-error "Method not recognized: ~S" arg))
           (common `(((*name*) ',name) ((*api*) '(*name* *api* *class* ,@(map car methods))) ((*class*) ',class)))
           (shared-methods
            '(deep-get deep-set! deep-slice! deep-prune! deep-merge! deep-copy!
              deep-call deep-call! serialize))
           (prep
            (lambda (method)
              (let ((direct
                     `(lambda args
                        (let ((state-before state))
                          (catch #t
                            (lambda ()
                              (apply ,(cadr method) (cons self args)))
                            (lambda error-args
                              (set! state state-before)
                              (apply throw error-args)))))))
                `((,(car method))
                  ,(if (and (eq? name 'standard)
                            (memq (car method) shared-methods))
                       `(lambda args
                          (if (sync-let-active?)
                              (apply ,direct args)
                              (let* ((payload
                                      (let loop ((in args) (ls '()) (expr '())
                                                 (nodes (sync-null)))
                                        (cond
                                         ((null? in)
                                          (sync-cons
                                           (expression->byte-vector (cons ls expr))
                                           nodes))
                                         ((sync-node? (car in))
                                          (loop (cdr in) '() (cons ls expr)
                                                (sync-cons (car in) nodes)))
                                         (else
                                          (loop (cdr in) (cons (car in) ls)
                                                expr nodes)))))
                                     (boundary
                                      (sync-let ((state state) (payload payload)
                                                 (method ',(car method)))
                                        (let* ((args
                                                (let loop
                                                    ((expr
                                                      (byte-vector->expression
                                                       (sync-car payload)))
                                                     (nodes (sync-cdr payload))
                                                     (out '()))
                                                  (if (null? (cdr expr))
                                                      (append (reverse (car expr)) out)
                                                      (loop
                                                       (cdr expr) (sync-cdr nodes)
                                                       (append
                                                        (cons (sync-car nodes)
                                                              (reverse (car expr)))
                                                        out)))))
                                               (portable (sync-eval state))
                                               (result
                                                (apply (portable method) args)))
                                          (list (portable) result)))))
                                (set! state (car boundary))
                                (cadr boundary))))
                       direct)))))
           (get '(lambda (node path)
                   (let loop ((node node) (path path))
                     (if (null? path) node
                         (if (zero? (car path))
                             (loop (sync-car node) (cdr path))
                             (loop (sync-cdr node) (cdr path)))))))
           (set '(lambda*
                  (arg-1 arg-2)
                  (set! state
                        (let loop ((node (self)) (path (if arg-2 arg-1 '())))
                          (if (null? path) (if arg-2 arg-2 arg-1)
                              (let ((node (if (sync-pair? node) node (sync-cons (sync-null) (sync-null)))))
                                (if (zero? (car path))
                                    (sync-cons (loop (sync-car node) (cdr path)) (sync-cdr node))
                                    (sync-cons (sync-car node) (loop (sync-cdr node) (cdr path)))))))) #t))
           (outer `(lambda (state)
                     (letrec*
                         ((self
                           (lambda* (func)
                             ,description
                             (set! (setter self) ,set)
                             (cond ((not func) state)
                                   ((list? func) (,get state func))
                                   (else (case func
                                           ,@common
                                           ,@(map prep methods)
                                           (else ,err)))))))
                       self))))
      (sync-cons (expression->byte-vector outer) (sync-null))))

  (define-method (init self class . init)
    ;; Build an object shell and run its *init* method with args.
    (let ((node ((self 'make) class)))
      (if (not (let loop ((body (cddr class)))
                 (cond ((null? body) #f)
                       ((string? (car body)) (loop (cdr body)))
                       ((eq? (caadar body) '*init*) #t)
                       (else (loop (cdr body))))))
          node
          (if (sync-let-active?)
              (let ((object (sync-eval node)))
                (apply (object '*init*) init)
                (object))
              (let ((payload
                 (let loop ((in init) (ls '()) (expr '())
                            (nodes (sync-null)))
                   (cond
                    ((null? in)
                     (sync-cons (expression->byte-vector (cons ls expr)) nodes))
                    ((sync-node? (car in))
                     (loop (cdr in) '() (cons ls expr)
                           (sync-cons (car in) nodes)))
                    (else
                     (loop (cdr in) (cons (car in) ls) expr nodes))))))
            (sync-let ((node node) (payload payload))
              (let* ((init
                      (let loop
                          ((expr
                            (byte-vector->expression (sync-car payload)))
                           (nodes (sync-cdr payload))
                           (out '()))
                        (if (null? (cdr expr))
                            (append (reverse (car expr)) out)
                            (loop
                             (cdr expr) (sync-cdr nodes)
                             (append
                              (cons (sync-car nodes) (reverse (car expr)))
                              out)))))
                     (object (sync-eval node)))
                (apply (object '*init*) init)
                (object))))))))

  (define-method (deep-get self object path)
    ;; Get value at path across nested nodes.
    (let ((object (sync-eval object)))
      (if (null? path) (object)
          (let ((node ((object 'get) (car path))))
            (cond ((equal? node '(nothing)) '(nothing))
                  ((equal? node '(unknown)) '(unknown))
                  ((not (sync-node? node))
                   (if (null? (cdr path)) node
                       (error 'path-error "Cannot continue deep-get through non-object value at path: ~S" path)))
                  ((sync-stub? node) '(unknown))
                  ((null? (cdr path)) node)
                  ((and (sync-pair? node) (byte-vector? (sync-car node)))
                   ((self 'deep-get) node (cdr path)))
                  (else (error 'path-error "Cannot continue deep-get through non-object value at path: ~S" path)))))))

  (define-method (deep-set! self object path value)
    ;; Set value at path across nested nodes.
    (let ((object (sync-eval object)))
      (if (= (length path) 1)
          (begin ((object 'set!) (car path) value)
                 (object))
          (let ((child ((self 'deep-set!) ((object 'get) (car path)) (cdr path) value)))
            ((object 'set!) (car path) child)
            (object)))))

  (define-method (deep-slice! self object path)
    ;; Slice node to retain proof along path.
    (let ((object (sync-eval object)))
      (if (= (length path) 1)
          (begin ((object 'slice!) (car path))
                 (object))
          (let* ((child ((object 'get) (car path)))
                 (result ((self 'deep-slice!) child (cdr path)))
                 (digest (sync-digest child)))
            (if (not (equal? (sync-digest result) digest))
                (error 'integrity-error "Slice changed digest at path: ~S" path))
            ((object 'set!) (car path) result)
            ((object 'slice!) (car path))
            (object)))))

  (define-method (deep-prune! self object path)
    ;; Prune node to remove proof along path.
    (let ((object (sync-eval object)))
      (if (= (length path) 1)
          (begin ((object 'prune!) (car path))
                 (object))
          (let ((child ((object 'get) (car path))))
            (if (not (and (sync-node? child) (sync-pair? child))) (object)
                (let* ((result ((self 'deep-prune!) child (cdr path)))
                       (digest (sync-digest child)))
                  (if (not (equal? (sync-digest result) digest))
                      (error 'integrity-error "Prune changed digest at path: ~S" path))
                  (if (sync-stub? result)
                      ((object 'prune!) (car path))
                      ((object 'set!) (car path) result))
                  (object)))))))

  (define-method (deep-merge! self object-source object-target (path '()))
    ;; Merge equivalent nodes by digest.
    (let ((merge-nodes
           (lambda (object-source object-target)
             (if (not (equal? (sync-digest object-source) (sync-digest object-target)))
                 (error 'integrity-error "Cannot merge objects with different digests: ~S vs ~S" (sync-digest object-source) (sync-digest object-target))
                 (let recurse ((node-1 object-source) (node-2 object-target))
                   (cond ((and (or (and (sync-node? node-1) (sync-node? node-2))
                                        (and (byte-vector? node-1) (byte-vector? node-2)))
                               (equal? node-1 node-2))
                          node-1)
                         ((sync-null? node-1) node-1)
                         ((byte-vector? node-1) node-1)
                         ((sync-stub? node-1) node-2)
                         ((sync-stub? node-2) node-1)
                         (else (sync-cons (recurse (sync-car node-1) (sync-car node-2))
                                          (recurse (sync-cdr node-1) (sync-cdr node-2))))))))))
      (if (null? path) (merge-nodes object-source object-target)
          ((self 'deep-set!) object-target path
           (merge-nodes object-source ((self 'deep-get) object-target path))))))

  (define-method (deep-copy! self object path-source path-target)
    ;; Copy value from source path to target path.
    ((self 'deep-set!) object path-target ((self 'deep-get) object path-source)))

  (define-method (deep-call self object path function)
    ;; Call function on node at path and return the callback result.
    (let ((object (sync-eval object)))
      (if (null? path)
          ((sync-let-eval function) object)
          (let* ((child ((object 'get) (car path)))
                 (result ((self 'deep-call) child (cdr path) function)))
            result))))

  (define-method (deep-call! self object path function)
    ;; Call function on node at path and rebuild the resulting node state.
    (let ((object (sync-eval object)))
      (if (null? path)
          (begin
            ((sync-let-eval function) object)
            (object))
          (let* ((child ((object 'get) (car path)))
                 (child ((self 'deep-call!) child (cdr path) function)))
            ((object 'set!) (car path) child)
            (object)))))

  (define-method (serialize self node query)
    ;; Serialize node with a traversal query into compact form.
    (sync-serialize node query))

  (define-method (deserialize self serialization)
    ;; Strictly deserialize sync-node data without evaluating input as Scheme.
    (sync-deserialize serialization)))
))
  (lambda* (operation (target class) (node #f))
    (case operation
      ((class) class)
      ((make)
       (let ((method (caddr class)))
         ((eval `(lambda* ,(cddadr method) ,@(cddr method))) target)))
      ((init)
       (let* ((trusted
               (let ((method (caddr class)))
         ((eval `(lambda* ,(cddadr method) ,@(cddr method))) target)))
              (object (sync-eval trusted))
              (init (and (member '*init* (object '*api*)) (object '*init*))))
         (if init (apply init (if node node '())))
         (object)))
      ((local)
       (if (not (sync-pair? node))
           (error 'object-error "Installed object state must be a sync pair"))
       (let ((trusted
              (let ((method (caddr class)))
         ((eval `(lambda* ,(cddadr method) ,@(cddr method))) target))))
         (sync-eval (sync-cons (sync-car trusted) (sync-cdr node)))))
      (else (error 'method-error "Standard module operation not recognized")))))
